mod kernel;

use std::{
  env, fs,
  io::{BufRead, BufReader},
  path::{Path, PathBuf},
  process::{Command, Stdio},
  time::Instant,
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, ValueEnum};
use kernel::{KERNELS, Kernel};
use serde::Deserialize;

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
        if target.name == "linux_live"
          && target.kind.iter().any(|kind| kind == "test") =>
      {
        executable
      },
      | Self::CompilerMessage {
        message: CargoDiagnostic {
          rendered: Some(rendered),
        },
      } => {
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
enum Profile {
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
  pub(crate) fn build_live_test() -> Result<()> {
    let repository = Self::repository()?;
    let cache = repository.join("tests/.cache/sakai-vmtest");
    Self::build_binary(&repository, &cache).map(|_| ())
  }

  fn repository() -> Result<PathBuf> {
    env::current_dir()?
      .ancestors()
      .find(|path| {
        path.join("xtask/Cargo.toml").is_file()
          && path.join("sakai-core/Cargo.toml").is_file()
      })
      .context("run vmtest from inside the Sakai repository")?
      .canonicalize()
      .map_err(Into::into)
  }

  pub(crate) async fn run(self) -> Result<()> {
    let repository = Self::repository()?;
    if env::consts::OS == "macos" {
      return self.run_in_container(&repository);
    }
    ensure!(
      env::consts::OS == "linux" && env::consts::ARCH == "x86_64",
      "vmtest requires an x86-64 Linux host"
    );
    let selected = KERNELS
      .iter()
      .filter(|kernel| match self.kernel.is_empty() {
        | false => self.kernel.iter().any(|name| name == kernel.name),
        | true => self.profile.matches(kernel),
      })
      .collect::<Vec<_>>();
    ensure!(
      !selected.is_empty(),
      "no kernel fixtures matched the request"
    );
    ensure!(
      self
        .kernel
        .iter()
        .all(|name| KERNELS.iter().any(|k| k.name == name)),
      "unknown kernel fixture requested"
    );

    let cache = repository.join("tests/.cache/sakai-vmtest");
    fs::create_dir_all(&cache)?;
    let supplied = self.test_binary.is_some();
    let binary = match self.test_binary {
      | Some(path) => path.canonicalize().context("test binary not found")?,
      | None => Self::build_binary(&repository, &cache)?,
    };
    // The config lives in cache, which vmtest shares at /mnt/vmtest.
    let guest_binary = cache.join("bin/linux_live");
    if binary != guest_binary {
      fs::create_dir_all(
        guest_binary.parent().context("binary has no parent")?,
      )?;
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
      "--test-binary must be a Linux executable; build it inside the \
       container instead"
    );
    let engine = match env::var_os("SAKAI_CONTAINER_ENGINE") {
      | Some(engine) => engine,
      | None => [
        ("container", &["system", "status"][..]),
        ("podman", &["info"][..]),
        ("docker", &["info"][..]),
      ]
      .into_iter()
      .find(|(engine, args)| {
        Command::new(engine)
          .args(*args)
          .output()
          .is_ok_and(|output| output.status.success())
      })
      .context(
        "install Apple Container, Podman, or Docker, or set \
         SAKAI_CONTAINER_ENGINE",
      )?
      .0
      .into(),
    };
    let image = "sakai-vmtest:vmtest-v0.18.0";
    let status = Command::new(&engine)
      .args(["build", "--platform", "linux/amd64", "-t", image, "-f"])
      .arg(repository.join("tools/vmtest/Containerfile"))
      .arg(repository)
      .status()
      .with_context(|| {
        format!("failed to build vmtest image with {engine:?}")
      })?;
    ensure!(status.success(), "vmtest image build failed: {status}");

    let mut run = Command::new(&engine);
    run.args(["run", "--rm", "--platform", "linux/amd64"]);
    run
      .arg("--volume")
      .arg(format!("{}:/workspace", repository.display()));
    run.args(["--workdir", "/workspace"]);
    for name in ["RUST_LOG", "CARGO_NET_OFFLINE"] {
      if let Some(value) = env::var_os(name) {
        run
          .arg("--env")
          .arg(format!("{name}={}", value.to_string_lossy()));
      }
    }
    run.args([
      "--env",
      "CARGO_HOME=/workspace/tests/.cache/sakai-vmtest/cargo",
    ]);
    run.args([
      "--env",
      "CARGO_TARGET_DIR=/workspace/tests/.cache/sakai-vmtest/target",
      image,
      "cargo",
      "xtask",
      "vmtest",
      "--profile",
      match self.profile {
        | Profile::Smoke => "smoke",
        | Profile::Full => "full",
      },
    ]);
    for kernel in self.kernel {
      run.args(["--kernel", &kernel]);
    }
    let status = run
      .status()
      .with_context(|| format!("failed to run vmtest image with {engine:?}"))?;
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
        "sakai-core",
        "--test",
        "linux_live",
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
    let executable = artifacts
      .into_iter()
      .flatten()
      .last()
      .context("Cargo did not report the linux_live executable")?;
    let destination = cache.join("bin/linux_live");
    fs::create_dir_all(destination.parent().context("binary has no parent")?)?;
    fs::copy(&executable, &destination)?;
    Self::record_binary(repository, cache, &destination)?;
    Ok(destination)
  }

  fn record_binary(
    repository: &Path,
    cache: &Path,
    binary: &Path,
  ) -> Result<()> {
    let digest = Kernel::digest(binary)?;
    let commit = Command::new("git")
      .current_dir(repository)
      .args(["rev-parse", "HEAD"])
      .output()?;
    ensure!(commit.status.success(), "failed to resolve Git commit");
    let commit = String::from_utf8(commit.stdout)?;
    fs::write(
      cache.join("bin/linux_live.meta"),
      format!("commit={}\nsha256={digest}\n", commit.trim()),
    )?;
    eprintln!("linux_live: {} (sha256 {digest})", binary.display());
    Ok(())
  }

  fn run_kernel(cache: &Path, kernel: &Kernel, image: &Path) -> Result<()> {
    let config = cache.join("vmtest.toml");
    let kernel_path = serde_json::to_string(&image.to_string_lossy().as_ref())?;
    let contents = format!(
      "[[target]]\nname = \"Linux {}\"\nkernel = {kernel_path}\nkernel_args = \
       \"ro\"\ncommand = \"findmnt -no OPTIONS / | grep -qw ro && test -w \
       /sys/fs/cgroup && SAKAI_VMTEST=1 /mnt/vmtest/bin/linux_live \
       --nocapture\"\n[target.vm]\nnum_cpus = 1\nmemory = \"1G\"\n",
      kernel.name
    );
    fs::write(&config, contents)?;
    eprintln!("==> Linux {}: SHA-256 {}", kernel.name, kernel.sha256);
    eprintln!("KVM acceleration: {}", Path::new("/dev/kvm").exists());
    let started = Instant::now();
    let status = Command::new("timeout")
      .arg("8m")
      .arg(
        env::var_os("SAKAI_VMTEST_EXECUTABLE")
          .unwrap_or_else(|| "vmtest-upstream".into()),
      )
      .arg("--config")
      .arg(&config)
      .env("VMTEST_NO_UI", "1")
      .status()
      .context("failed to start vmtest (enter devenv shell first)")?;
    eprintln!(
      "Linux {} finished in {:.1}s: {status}",
      kernel.name,
      started.elapsed().as_secs_f64()
    );
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
      let message = serde_json::from_str::<CargoMessage>(json)
        .expect("Cargo JSON should deserialize");
      assert_eq!(message.executable(), expected);
    }
  }
}
