use std::{
  fs,
  path::{self, Path, PathBuf},
  process::{Command, Stdio},
};

use anyhow::{Context, Result, ensure};
use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct InstallationSize {
  /// Clean Git checkout to package.
  #[arg(long, default_value = ".")]
  checkout: PathBuf,
  /// Path to the Python interpreter used to build and download the wheel.
  #[arg(long)]
  python: PathBuf,
}

#[derive(Default)]
struct Downloads {
  count: usize,
  bytes: u64,
}

impl Downloads {
  fn add(&mut self, archive: &Path) -> Result<()> {
    self.count = self.count.checked_add(1).context("too many dependency archives")?;
    self.bytes = self.bytes.checked_add(fs::metadata(archive)?.len()).context("dependency archives are too large")?;
    Ok(())
  }
}

impl InstallationSize {
  pub(crate) fn run(self) -> Result<()> {
    let checkout = path::absolute(self.checkout)?;
    let python = path::absolute(self.python)?;
    let status = Self::command(Command::new("git").current_dir(&checkout).args(["status", "--porcelain"]))?;
    ensure!(status.is_empty(), "installation size requires a clean checkout at {}", checkout.display());
    let commit = Self::command(Command::new("git").current_dir(&checkout).args(["rev-parse", "HEAD"]))?;
    let scratch = tempfile::tempdir()?;
    let (crate_bytes, rust) = Self::rust_downloads(&checkout, scratch.path())?;
    let (wheel_bytes, python) = Self::python_downloads(&checkout, &python, scratch.path())?;

    println!(
      "commit,rust_crate_bytes,rust_dependency_count,rust_dependency_bytes,python_wheel_bytes,python_dependency_count,\
       python_dependency_bytes"
    );
    println!(
      "{},{crate_bytes},{},{},{wheel_bytes},{},{}",
      commit.trim(),
      rust.count,
      rust.bytes,
      python.count,
      python.bytes
    );
    Ok(())
  }

  fn rust_downloads(checkout: &Path, scratch: &Path) -> Result<(u64, Downloads)> {
    let packaging_target = scratch.join("package-target");
    Self::command(
      Command::new("cargo")
        .current_dir(checkout)
        .args(["package", "--package", "sakai", "--locked", "--no-verify", "--target-dir"])
        .arg(&packaging_target),
    )?;
    let archive = Self::only_archive(&packaging_target.join("package"), "crate")?;
    let package = scratch.join(archive.file_stem().context("crate archive has no package directory")?);
    Self::command(Command::new("tar").current_dir(scratch).arg("-xzf").arg(&archive))?;

    let consumer = scratch.join("consumer");
    fs::create_dir_all(consumer.join("src"))?;
    fs::write(consumer.join("src/main.rs"), "fn main() {}\n")?;
    fs::write(
      consumer.join("Cargo.toml"),
      format!(
        "[package]\nname = 'sakai-size-consumer'\nversion = '0.0.0'\nedition = '2024'\n[dependencies]\nsakai = {{ \
         path = {} }}\n",
        serde_json::to_string(&package)?,
      ),
    )?;
    // Seed the downstream resolution with the versions in the packaged crate.
    fs::copy(package.join("Cargo.lock"), consumer.join("Cargo.lock"))?;
    let cargo_home = scratch.join("cargo-home");
    // Check the actual default-feature build so unused optional crates in the lockfile aren't downloaded.
    Self::command(
      Command::new("cargo")
        .current_dir(&consumer)
        .env("CARGO_HOME", &cargo_home)
        .args(["check", "--target", "host-tuple", "--target-dir"])
        .arg(consumer.join("target")),
    )?;

    let cache = cargo_home.join("registry/cache");
    let mut dependencies = Downloads::default();
    if cache.try_exists()? {
      for registry in fs::read_dir(cache)? {
        for archive in Self::archives(&registry?.path(), "crate")? {
          dependencies.add(&archive)?;
        }
      }
    }
    Ok((fs::metadata(archive)?.len(), dependencies))
  }

  fn python_downloads(checkout: &Path, python: &Path, scratch: &Path) -> Result<(u64, Downloads)> {
    let wheels = scratch.join("wheels");
    Self::command(
      Command::new("maturin")
        .current_dir(checkout)
        .env_remove("LD_LIBRARY_PATH")
        .env("PYO3_PYTHON", python)
        .args(["build", "--release", "--strip", "--locked", "--manifest-path", "sakai-python/Cargo.toml", "--out"])
        .arg(&wheels)
        // A target directory shared by checkouts can reuse a different revision's native library.
        .arg("--target-dir")
        .arg(scratch.join("wheel-target"))
        .arg("--interpreter")
        .arg(python),
    )?;
    let wheel = Self::only_archive(&wheels, "whl")?;
    let downloads = scratch.join("downloads");
    let mut pip = Command::new(python);
    pip
      .current_dir(scratch)
      .env_remove("LD_LIBRARY_PATH")
      .args(["-m", "pip", "--isolated", "--disable-pip-version-check", "download", "--no-cache-dir", "--dest"])
      .arg(&downloads)
      .arg(&wheel);
    let project = checkout.join("sakai-python");
    if project.join("uv.lock").try_exists()? {
      let constraints = Self::command(
        Command::new("uv")
          .current_dir(&project)
          .env_remove("LD_LIBRARY_PATH")
          .args([
            "export",
            "--locked",
            "--no-default-groups",
            "--no-emit-project",
            "--no-hashes",
            "--no-annotate",
            "--no-header",
            "--python",
          ])
          .arg(python),
      )?;
      let constraints_path = scratch.join("constraints.txt");
      fs::write(&constraints_path, constraints)?;
      pip.arg("--constraint").arg(constraints_path);
    }
    Self::command(&mut pip)?;

    let filename = wheel.file_name().context("wheel has no filename")?;
    let mut dependencies = Downloads::default();
    for archive in fs::read_dir(&downloads)? {
      let archive = archive?;
      if archive.file_name() != filename {
        dependencies.add(&archive.path())?;
      }
    }
    Ok((fs::metadata(downloads.join(filename))?.len(), dependencies))
  }

  fn archives(directory: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let entries =
      fs::read_dir(directory)?.map(|entry| entry.map(|entry| entry.path())).collect::<std::io::Result<Vec<_>>>()?;
    Ok(entries.into_iter().filter(|path| path.extension().is_some_and(|value| value == extension)).collect())
  }

  fn only_archive(directory: &Path, extension: &str) -> Result<PathBuf> {
    let mut archives = Self::archives(directory, extension)?;
    ensure!(archives.len() == 1, "expected one .{extension} archive in {}", directory.display());
    archives.pop().context("missing package archive")
  }

  fn command(command: &mut Command) -> Result<String> {
    let result = command.stderr(Stdio::inherit()).output().with_context(|| format!("run {command:?}"))?;
    let stdout = String::from_utf8(result.stdout)?;
    ensure!(result.status.success(), "{command:?} failed with {}\n{stdout}", result.status);
    Ok(stdout)
  }
}
