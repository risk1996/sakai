use std::{
  env, fs,
  io::{BufRead, BufReader},
  path::{Path, PathBuf},
  process::{Command, Stdio},
  time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, ValueEnum};
use kernel::Kernel;
use serde::Deserialize;

mod kernel;

#[derive(Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
enum CargoMessage {
  CompilerArtifact {
    target: CargoTarget,
    executable: Option<PathBuf>,
  },
  CompilerMessage {
    message: CargoDiagnostic,
  },
  #[serde(other)]
  Other,
}

#[derive(Deserialize)]
struct CargoTarget {
  name: String,
  kind: Vec<String>,
}

#[derive(Deserialize)]
struct CargoDiagnostic {
  rendered: Option<String>,
}

impl CargoMessage {
  fn executable(self) -> Option<PathBuf> {
    match self {
      | Self::CompilerArtifact { target, executable }
        if target.name == Vmtest::LIVE_TEST_TARGET && target.kind.iter().any(|kind| kind == "test") =>
      {
        executable
      },
      | Self::CompilerMessage { message: CargoDiagnostic { rendered: Some(rendered) } } => {
        eprint!("{rendered}");
        None
      },
      | _ => None,
    }
  }
}

#[derive(Debug, Args)]
pub(crate) struct Vmtest {
  /// Choose the quick or complete kernel matrix.
  #[arg(long, value_enum, default_value_t = Profile::Smoke)]
  profile: Profile,
  /// Run only the named kernel; repeat to select more than one.
  #[arg(long)]
  kernel: Vec<String>,
  /// Run an existing host-built linux_live executable without invoking Cargo.
  #[arg(long)]
  test_binary: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Profile {
  Smoke,
  Full,
}

impl Profile {
  fn matches(self, kernel: &Kernel) -> bool {
    match self {
      | Self::Smoke => kernel.smoke,
      | Self::Full => true,
    }
  }
}

impl Vmtest {
  const BINARY_PATH: &str = "bin/linux_live";
  const CACHE_DIRECTORY: &str = "tests/.cache/sakai-vmtest";
  const CONFIG_FILE_NAME: &str = "vmtest.toml";
  const CONTAINER_IMAGE: &str = "sakai-vmtest:devenv";
  const CONTAINER_PLATFORM: &str = "linux/amd64";
  const CONTAINER_WORKDIR: &str = "/workspace";
  const EXECUTABLE: &str = "vmtest-upstream";
  const GUEST_SHARE_DIRECTORY: &str = "/mnt/vmtest";
  const LIVE_TEST_TARGET: &str = "linux_live";
  const METADATA_PATH: &str = "bin/linux_live.meta";
  const RUN_TIMEOUT: Duration = Duration::from_secs(480);

  pub(crate) fn kernel_matrix(profile: Profile) -> Result<()> {
    let names =
      Kernel::ALL.iter().filter(|kernel| profile.matches(kernel)).map(|kernel| kernel.name).collect::<Vec<_>>();
    println!("{}", serde_json::to_string(&names)?);
    Ok(())
  }

  pub(crate) fn build_live_test() -> Result<()> {
    let repository = Self::repository()?;
    let cache = repository.join(Self::CACHE_DIRECTORY);
    Self::build_binary(&repository, &cache).map(|_| ())
  }

  fn repository() -> Result<PathBuf> {
    env::current_dir()?
      .ancestors()
      .find(|path| path.join("xtask/Cargo.toml").is_file() && path.join("sakai/Cargo.toml").is_file())
      .context("run vmtest from inside the Sakai repository")?
      .canonicalize()
      .map_err(Into::into)
  }

  pub(crate) async fn run(self) -> Result<()> {
    let repository = Self::repository()?;
    if env::consts::OS == "macos" {
      return self.run_in_container(&repository);
    }
    ensure!(env::consts::OS == "linux" && env::consts::ARCH == "x86_64", "vmtest requires an x86-64 Linux host");
    let selected = Kernel::ALL
      .iter()
      .filter(|kernel| match self.kernel.is_empty() {
        | false => self.kernel.iter().any(|name| name == kernel.name),
        | true => self.profile.matches(kernel),
      })
      .collect::<Vec<_>>();
    ensure!(!selected.is_empty(), "no kernel fixtures matched the request");
    ensure!(
      self.kernel.iter().all(|name| Kernel::ALL.iter().any(|k| k.name == name)),
      "unknown kernel fixture requested"
    );

    let cache = repository.join(Self::CACHE_DIRECTORY);
    fs::create_dir_all(&cache)?;
    let supplied = self.test_binary.is_some();
    let binary = match self.test_binary {
      | Some(path) => path.canonicalize().context("test binary not found")?,
      | None => Self::build_binary(&repository, &cache)?,
    };
    // The config lives in cache, which vmtest shares at /mnt/vmtest.
    let guest_binary = cache.join(Self::BINARY_PATH);
    if binary != guest_binary {
      fs::create_dir_all(guest_binary.parent().context("binary has no parent")?)?;
      fs::copy(&binary, &guest_binary)?;
    }
    if supplied {
      Self::record_binary(&repository, &cache, &guest_binary)?;
    }
    for kernel in selected {
      let image = kernel.image(&cache.join("kernels")).await?;
      Self::run_kernel(&cache, kernel, &image)?;
    }
    Ok(())
  }

  fn run_in_container(self, repository: &Path) -> Result<()> {
    ensure!(
      self.test_binary.is_none(),
      "--test-binary must be a Linux executable; build it inside the container instead"
    );
    let engine = match env::var_os("SAKAI_CONTAINER_ENGINE") {
      | Some(engine) => engine,
      | None => [("container", &["system", "status"][..]), ("podman", &["info"][..]), ("docker", &["info"][..])]
        .into_iter()
        .find(|(engine, args)| Command::new(engine).args(*args).output().is_ok_and(|output| output.status.success()))
        .context("install Apple Container, Podman, or Docker, or set SAKAI_CONTAINER_ENGINE")?
        .0
        .into(),
    };
    let rust_version =
      env::var("SAKAI_VMTEST_RUST_VERSION").context("enter devenv shell to resolve the container Rust version")?;
    let vmtest_url = env::var("SAKAI_VMTEST_URL").context("enter devenv shell to resolve the vmtest download URL")?;
    let image = Self::CONTAINER_IMAGE;
    let status = Command::new(&engine)
      .args(["build", "--platform", Self::CONTAINER_PLATFORM])
      .args(["--build-arg", &format!("RUST_IMAGE=rust:{rust_version}-bookworm")])
      .args(["--build-arg", &format!("VMTEST_URL={vmtest_url}")])
      .args(["-t", image, "-f"])
      .arg(repository.join("tools/vmtest/Containerfile"))
      .arg(repository)
      .status()
      .with_context(|| format!("failed to build vmtest image with {engine:?}"))?;
    ensure!(status.success(), "vmtest image build failed: {status}");

    let mut run = Command::new(&engine);
    run.args(["run", "--rm", "--platform", Self::CONTAINER_PLATFORM]);
    run.arg("--volume").arg(format!("{}:{}", repository.display(), Self::CONTAINER_WORKDIR));
    run.args(["--workdir", Self::CONTAINER_WORKDIR]);
    for name in ["RUST_LOG", "CARGO_NET_OFFLINE"] {
      if let Some(value) = env::var_os(name) {
        run.arg("--env").arg(format!("{name}={}", value.to_string_lossy()));
      }
    }
    let container_cache = Path::new(Self::CONTAINER_WORKDIR).join(Self::CACHE_DIRECTORY);
    for (name, directory) in [("CARGO_HOME", "cargo"), ("CARGO_TARGET_DIR", "target")] {
      run.args(["--env", &format!("{name}={}", container_cache.join(directory).display())]);
    }
    run.args([image, "cargo", "xtask", "vmtest", "--profile", match self.profile {
      | Profile::Smoke => "smoke",
      | Profile::Full => "full",
    }]);
    for kernel in self.kernel {
      run.args(["--kernel", &kernel]);
    }
    let status = run.status().with_context(|| format!("failed to run vmtest image with {engine:?}"))?;
    ensure!(status.success(), "containerized vmtest failed: {status}");
    Ok(())
  }

  fn build_binary(repository: &Path, cache: &Path) -> Result<PathBuf> {
    let mut cargo = Command::new("cargo")
      .current_dir(repository)
      .args([
        "test",
        "--locked",
        "--package",
        "sakai",
        "--test",
        Self::LIVE_TEST_TARGET,
        "--no-run",
        "--message-format=json-render-diagnostics",
      ])
      .stdout(Stdio::piped())
      .spawn()
      .context("failed to start Cargo")?;
    let output = cargo.stdout.take().context("Cargo stdout unavailable")?;
    let artifacts = BufReader::new(output)
      .lines()
      .map(|line| -> Result<Option<PathBuf>> {
        let message: CargoMessage = serde_json::from_str(&line?)?;
        Ok(message.executable())
      })
      .collect::<Result<Vec<_>>>()?;
    ensure!(cargo.wait()?.success(), "Cargo failed to build linux_live");
    let executable =
      artifacts.into_iter().flatten().last().context("Cargo did not report the linux_live executable")?;
    let destination = cache.join(Self::BINARY_PATH);
    fs::create_dir_all(destination.parent().context("binary has no parent")?)?;
    fs::copy(&executable, &destination)?;
    Self::record_binary(repository, cache, &destination)?;
    Ok(destination)
  }

  fn record_binary(repository: &Path, cache: &Path, binary: &Path) -> Result<()> {
    let commit = Command::new("git").current_dir(repository).args(["rev-parse", "HEAD"]).output()?;
    ensure!(commit.status.success(), "failed to resolve Git commit");
    let commit = String::from_utf8(commit.stdout)?;
    fs::write(cache.join(Self::METADATA_PATH), format!("commit={}\n", commit.trim()))?;
    eprintln!("linux_live: {}", binary.display());
    Ok(())
  }

  fn run_kernel(cache: &Path, kernel: &Kernel, image: &Path) -> Result<()> {
    let config = cache.join(Self::CONFIG_FILE_NAME);
    let kernel_path = serde_json::to_string(&image.to_string_lossy().as_ref())?;
    let guest_binary = Path::new(Self::GUEST_SHARE_DIRECTORY).join(Self::BINARY_PATH);
    let contents = format!(
      "[[target]]\nname = \"Linux {}\"\nkernel = {kernel_path}\nkernel_args = \"ro\"\ncommand = \"findmnt -no OPTIONS \
       / | grep -qw ro && test -w /sys/fs/cgroup && SAKAI_VMTEST=1 {} --nocapture\"\n[target.vm]\nnum_cpus = \
       1\nmemory = \"1G\"\n",
      kernel.name,
      guest_binary.display(),
    );
    fs::write(&config, contents)?;
    eprintln!("==> Linux {}", kernel.name);
    eprintln!("KVM acceleration: {}", Path::new("/dev/kvm").exists());
    let started = Instant::now();
    let status = Command::new("timeout")
      .arg(format!("{}s", Self::RUN_TIMEOUT.as_secs()))
      .arg(env::var_os("SAKAI_VMTEST_EXECUTABLE").unwrap_or_else(|| Self::EXECUTABLE.into()))
      .arg("--config")
      .arg(&config)
      .env("VMTEST_NO_UI", "1")
      .status()
      .context("failed to start vmtest (enter devenv shell first)")?;
    eprintln!("Linux {} finished in {:.1}s: {status}", kernel.name, started.elapsed().as_secs_f64());
    match status.code() {
      | Some(0) => Ok(()),
      | Some(code) => std::process::exit(code),
      | None => bail!("vmtest terminated by signal"),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use super::CargoMessage;

  #[test]
  fn selects_only_linux_live_test_executable() {
    let cases = [
      (
        r#"{"reason":"compiler-artifact","target":{"name":"linux_live","kind":["test"]},"executable":"/tmp/linux_live"}"#,
        Some(PathBuf::from("/tmp/linux_live")),
      ),
      (
        r#"{"reason":"compiler-artifact","target":{"name":"linux_live","kind":["bin"]},"executable":"/tmp/wrong-kind"}"#,
        None,
      ),
      (
        r#"{"reason":"compiler-artifact","target":{"name":"other","kind":["test"]},"executable":"/tmp/wrong-target"}"#,
        None,
      ),
      (r#"{"reason":"build-finished","success":true}"#, None),
    ];

    for (json, expected) in cases {
      let message = serde_json::from_str::<CargoMessage>(json).expect("Cargo JSON should deserialize");
      assert_eq!(message.executable(), expected);
    }
  }
}
