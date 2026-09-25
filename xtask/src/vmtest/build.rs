use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

use super::kernel::Kernel;

const BUILT_INS: &[&str] = &[
  "CONFIG_EXT4_FS=y",
  "CONFIG_FUSE_FS=y",
  "CONFIG_OVERLAY_FS=y",
  "CONFIG_VIRTIO=y",
  "CONFIG_VIRTIO_BLK=y",
  "CONFIG_VIRTIO_CONSOLE=y",
  "CONFIG_VIRTIO_FS=y",
  "CONFIG_VIRTIO_NET=y",
  "CONFIG_VIRTIO_PCI=y",
  "CONFIG_VIRTIO_VSOCKETS=y",
  "CONFIG_VIRTIO_VSOCKETS_COMMON=y",
  "CONFIG_VSOCKETS=y",
];

impl Kernel {
  pub(super) async fn build(&self, cache: &Path) -> Result<()> {
    let Some(source_sha256) = self.source_sha256 else {
      return Ok(());
    };
    ensure!(
      cfg!(all(target_os = "linux", target_arch = "x86_64")),
      "kernel builds require an x86_64 Linux host"
    );
    eprintln!(
      "Building Linux {} from kernel.org source SHA-256 {}",
      self.name, source_sha256
    );

    let image = self.built_image(cache);
    let build = cache
      .parent()
      .context("kernel cache has no parent")?
      .join("kernel-build")
      .join(self.name);
    let output = build.join("output");
    let config = output.join(".config");
    let cached_config = cache.join(format!("config-v{}-boxlite", self.name));
    if image.is_file() && cached_config.is_file() {
      Self::verify_built_ins(&fs::read_to_string(&cached_config)?)?;
      eprintln!("Reusing built kernel {}", image.display());
      return Ok(());
    }

    let fixture = self
      .fixture(&cache.join("fixtures"))
      .await?
      .context("kernel fixture is required for its baseline config")?;
    let baseline = Self::embedded_config(&fs::read(&fixture)?)
      .context("vmtest fixture has no embedded kernel config")?;
    fs::create_dir_all(&output)?;
    fs::write(&config, baseline)?;

    let archive = build.join(format!("linux-{}.tar.xz", self.name));
    if !archive.is_file() {
      let version = if self.name.starts_with('5') {
        "v5.x"
      } else {
        "v6.x"
      };
      let url = format!(
        "https://cdn.kernel.org/pub/linux/kernel/{version}/linux-{}.tar.xz",
        self.name
      );
      eprintln!("Downloading pinned kernel source: {url}");
      let source = reqwest::get(&url)
        .await?
        .error_for_status()?
        .bytes()
        .await?;
      ensure!(
        hex::encode(Sha256::digest(&source)) == source_sha256,
        "kernel.org source checksum mismatch for {url}"
      );
      fs::write(&archive, source)?;
    } else {
      ensure!(
        hex::encode(Sha256::digest(fs::read(&archive)?)) == source_sha256,
        "cached kernel source checksum mismatch: {}",
        archive.display()
      );
    }

    let source_dir = build.join(format!("linux-{}", self.name));
    let extracted = source_dir.join(".sakai-extracted");
    if !extracted.is_file() {
      Self::run(
        Command::new("tar")
          .arg("-xJf")
          .arg(&archive)
          .arg("-C")
          .arg(&build),
      )?;
      fs::write(&extracted, [])?;
    }

    let mut options = Command::new(source_dir.join("scripts/config"));
    options.arg("--file").arg(&config);
    for option in [
      "EXT4_FS",
      "FUSE_FS",
      "OVERLAY_FS",
      "VIRTIO",
      "VIRTIO_BLK",
      "VIRTIO_CONSOLE",
      "VIRTIO_FS",
      "VIRTIO_NET",
      "VIRTIO_PCI",
      "VIRTIO_VSOCKETS",
      "VSOCKETS",
      "IKCONFIG",
    ] {
      options.arg("--enable").arg(option);
    }
    for option in [
      "DEBUG_INFO",
      "DEBUG_INFO_BTF",
      "GCC_PLUGINS",
      "MODULE_SIG",
      "MODULE_SIG_ALL",
      "LOCALVERSION_AUTO",
      "WERROR",
    ] {
      options.arg("--disable").arg(option);
    }
    options
      .arg("--set-str")
      .arg("SYSTEM_TRUSTED_KEYS")
      .arg("")
      .arg("--set-str")
      .arg("SYSTEM_REVOCATION_KEYS")
      .arg("")
      .arg("--set-str")
      .arg("LOCALVERSION")
      .arg("-sakai");
    Self::run(&mut options)?;

    Self::run(
      Command::new("make")
        .arg(format!("O={}", output.display()))
        .arg("olddefconfig")
        .current_dir(&source_dir),
    )?;
    Self::verify_built_ins(&fs::read_to_string(&config)?)?;
    Self::run(
      Command::new("make")
        .arg("-s")
        .arg(format!("O={}", output.display()))
        .arg("kernelrelease")
        .current_dir(&source_dir),
    )?;

    let jobs = std::thread::available_parallelism()?.get();
    eprintln!("Compiling with {jobs} parallel jobs");
    Self::run(
      Command::new("make")
        .arg(format!("-j{jobs}"))
        .arg(format!("O={}", output.display()))
        .arg("bzImage")
        .current_dir(&source_dir),
    )?;
    fs::create_dir_all(cache)?;
    let compiled = output.join("arch/x86/boot/bzImage");
    ensure!(
      compiled.is_file(),
      "kernel build did not produce {}",
      compiled.display()
    );
    fs::copy(&compiled, &image)?;
    fs::copy(&config, &cached_config)?;
    Self::report_boot_config(&image)?;
    Ok(())
  }

  fn verify_built_ins(config: &str) -> Result<()> {
    for option in BUILT_INS {
      ensure!(
        config.lines().any(|line| line == *option),
        "required BoxLite boot driver is not built in: {option}"
      );
    }
    Ok(())
  }

  fn run(command: &mut Command) -> Result<()> {
    eprintln!("Running {command:?}");
    ensure!(command.status()?.success(), "command failed: {command:?}");
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::{BUILT_INS, Kernel};

  #[test]
  fn verifies_every_boot_driver_is_built_in() {
    let config = BUILT_INS.join("\n");
    Kernel::verify_built_ins(&config).unwrap();

    for missing in BUILT_INS {
      let incomplete = config
        .lines()
        .filter(|line| line != missing)
        .collect::<Vec<_>>()
        .join("\n");
      let error = Kernel::verify_built_ins(&incomplete).unwrap_err();
      assert!(error.to_string().contains(missing));
    }
  }
}
