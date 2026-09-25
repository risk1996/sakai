use std::{env, fs, path::Path};

use anyhow::Result;

use super::kernel::Kernel;

pub(super) struct Diagnostics;

impl Diagnostics {
  pub(super) fn collect(repository: &Path) -> Result<()> {
    let cache = repository.join("tests/.cache/sakai-vmtest");
    let run_id = env::var("GITHUB_RUN_ID")
      .ok()
      .filter(|id| {
        !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())
      })
      .unwrap_or_else(|| format!("local-{}", std::process::id()));
    let staging = cache.join("diagnostics").join(run_id);
    let boxlite = cache.join("boxlite");
    let architecture = env::consts::ARCH;

    for (name, home) in [
      ("bundled-smoke", boxlite.join("bundled-smoke")),
      ("boot-probe", boxlite.join(architecture).join("boot-probe")),
      ("test", boxlite.join(architecture).join("test")),
    ] {
      Self::copy_home(&home, &staging.join(name))?;
    }
    let kernels = cache.join("kernels");
    if kernels.is_dir() {
      let destination = staging.join("kernels");
      fs::create_dir_all(&destination)?;
      for image in fs::read_dir(kernels)? {
        let image = image?;
        if image.file_type()?.is_file() {
          let name = image.file_name().to_string_lossy().into_owned();
          if name.starts_with("bzImage-") || name.starts_with("vmlinux-") {
            Kernel::export_config(
              &image.path(),
              &destination.join(format!("{name}.config")),
            )?;
          } else if name.starts_with("config-v") {
            Self::copy_file(&image.path(), &destination.join(name))?;
          }
        }
      }
    }
    let builds = cache.join("kernel-build");
    if builds.is_dir() {
      for entry in fs::read_dir(builds)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
          let destination = staging.join("builds").join(entry.file_name());
          Self::copy_file(
            &entry.path().join("output/.config"),
            &destination.join("kernel.config"),
          )?;
          Self::copy_file(
            &entry.path().join("output/include/config/kernel.release"),
            &destination.join("kernel.release"),
          )?;
        }
      }
    }
    eprintln!("Staged BoxLite logs in {}", staging.display());
    Ok(())
  }

  fn copy_home(home: &Path, staging: &Path) -> Result<()> {
    if !home.is_dir() {
      return Ok(());
    }

    Self::copy_regular_files(
      &home.join("logs"),
      &staging.join("runtime-logs"),
    )?;
    let boxes = home.join("boxes");
    if !boxes.is_dir() {
      return Ok(());
    }
    for box_dir in fs::read_dir(boxes)? {
      let box_dir = box_dir?;
      if !box_dir.file_type()?.is_dir() {
        continue;
      }
      let source = box_dir.path();
      let destination = staging.join("boxes").join(box_dir.file_name());
      for name in ["shim.stderr", "exit"] {
        Self::copy_file(&source.join(name), &destination.join(name))?;
      }
      Self::copy_regular_files(
        &source.join("logs"),
        &destination.join("logs"),
      )?;
    }
    Ok(())
  }

  fn copy_regular_files(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_dir() {
      return Ok(());
    }
    for entry in fs::read_dir(source)? {
      let entry = entry?;
      if entry.file_type()?.is_file() {
        Self::copy_file(&entry.path(), &destination.join(entry.file_name()))?;
      }
    }
    Ok(())
  }

  fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file() {
      return Ok(());
    }
    if let Some(parent) = destination.parent() {
      fs::create_dir_all(parent)?;
    }
    match fs::copy(source, destination) {
      | Ok(bytes) => eprintln!("Copied {} ({} bytes)", source.display(), bytes),
      | Err(error) => eprintln!("Could not copy {}: {error}", source.display()),
    }
    Ok(())
  }
}
