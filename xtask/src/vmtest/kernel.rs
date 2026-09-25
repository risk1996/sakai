use std::{
  fs,
  io::Read,
  path::{Path, PathBuf},
};

use anyhow::{Result, ensure};
use clap::ValueEnum;
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(super) enum KernelImageFormat {
  #[default]
  Elf,
  Compressed,
}

impl KernelImageFormat {
  const fn file_stem(self) -> &'static str {
    match self {
      | Self::Elf => "vmlinux",
      | Self::Compressed => "bzImage",
    }
  }
}

pub(super) const KERNELS: &[Kernel] = &[
  Kernel {
    name: "native",
    architecture: "aarch64",
    smoke: true,
    sha256: None,
    source_sha256: None,
  },
  Kernel::x86_64(
    "5.15",
    true,
    "23879f21c7e3c7137902904fd89695fc9d8a938f1d2c1549dd1658443cb7a084",
    "57b2cf6991910e3b67a1b3490022e8a0674b6965c74c12da1e99d138d1991ee8",
  ),
  Kernel::x86_64(
    "6.1",
    false,
    "303b9010d92e4a9cf3930114f5c854edb2c5f7d1f9da2c3c29df7b1f7ab86a3c",
    "2ca1f17051a430f6fed1196e4952717507171acfd97d96577212502703b25deb",
  ),
  Kernel::x86_64(
    "6.6",
    false,
    "3b47b1fefe02d49208da139bb1ad0363971ca5c02ab4ce89b9b8a52504b0fedf",
    "d926a06c63dd8ac7df3f86ee1ffc2ce2a3b81a2d168484e76b5b389aba8e56d0",
  ),
  Kernel::x86_64(
    "6.12",
    false,
    "a389c774c4bf035fbb7685be8493d128d8196c62862f750a8ef49b3b58428738",
    "b1a2562be56e42afb3f8489d4c2a7ac472ac23098f1ef1c1e40da601f54625eb",
  ),
  Kernel::x86_64(
    "6.18",
    true,
    "c2a05883f9556e73f5665d5979679163d80ff17754c4090e8d801df2a2e92551",
    "9106a4605da9e31ff17659d958782b815f9591ab308d03b0ee21aad6c7dced4b",
  ),
];

#[derive(Debug)]
pub(super) struct Kernel {
  pub(super) name: &'static str,
  pub(super) architecture: &'static str,
  pub(super) smoke: bool,
  pub(super) sha256: Option<&'static str>,
  pub(super) source_sha256: Option<&'static str>,
}

impl Kernel {
  pub(super) fn report_boot_config(path: &Path) -> Result<()> {
    const OPTIONS: &[&str] = &[
      "CONFIG_BLK_DEV_INITRD",
      "CONFIG_CGROUPS",
      "CONFIG_DEVTMPFS",
      "CONFIG_EXT4_FS",
      "CONFIG_FUSE_FS",
      "CONFIG_INET",
      "CONFIG_MODULES",
      "CONFIG_NAMESPACES",
      "CONFIG_OVERLAY_FS",
      "CONFIG_SERIAL_8250_CONSOLE",
      "CONFIG_USER_NS",
      "CONFIG_VIRTIO",
      "CONFIG_VIRTIO_BLK",
      "CONFIG_VIRTIO_CONSOLE",
      "CONFIG_VIRTIO_FS",
      "CONFIG_VIRTIO_NET",
      "CONFIG_VIRTIO_PCI",
      "CONFIG_VIRTIO_VSOCKETS",
      "CONFIG_VSOCKETS",
    ];

    let image = fs::read(path)?;
    eprintln!("Kernel image: {} ({} bytes)", path.display(), image.len());
    eprintln!("Kernel SHA-256: {}", hex::encode(Sha256::digest(&image)));
    let Some(config) = Self::embedded_config(&image) else {
      eprintln!("Embedded kernel config: unavailable");
      return Ok(());
    };
    eprintln!("Embedded kernel boot configuration:");
    for option in OPTIONS {
      let disabled = format!("# {option} is not set");
      match config.lines().find(|line| {
        line
          .strip_prefix(option)
          .is_some_and(|value| value.starts_with('='))
          || *line == disabled
      }) {
        | Some(line) => eprintln!("  {line}"),
        | None => eprintln!("  {option}: unavailable"),
      }
    }
    for option in [
      "CONFIG_EXT4_FS=y",
      "CONFIG_VIRTIO_BLK=y",
      "CONFIG_VIRTIO_VSOCKETS=y",
    ] {
      if !config.lines().any(|line| line == option) {
        eprintln!(
          "BOOT WARNING: {option} is not built in; this fixture has no \
           matching initramfs"
        );
      }
    }
    Ok(())
  }

  pub(super) fn embedded_config(image: &[u8]) -> Option<String> {
    const ZSTD_MAGIC: &[u8] = &[0x28, 0xb5, 0x2f, 0xfd];

    if image.starts_with(b"\x7fELF") {
      return Self::config_in_payload(image);
    }

    let unpacked = image
      .windows(ZSTD_MAGIC.len())
      .position(|bytes| bytes == ZSTD_MAGIC)
      .and_then(|offset| {
        image.get(offset..).map(|compressed| (offset, compressed))
      })
      .map(|(offset, compressed)| {
        eprintln!("Embedded Zstd kernel payload at offset {offset}");
        let mut unpacked = Vec::new();
        let result = zstd::stream::copy_decode(compressed, &mut unpacked);
        eprintln!(
          "Unpacked kernel payload: {} bytes, ELF={}",
          unpacked.len(),
          unpacked.starts_with(b"\x7fELF")
        );
        (unpacked, result)
      });
    if let Some((payload, result)) = unpacked {
      if let Some(config) = Self::config_in_payload(&payload) {
        return Some(config);
      }
      if let Err(error) = result {
        eprintln!("Kernel payload decompression warning: {error}");
      }
    }
    Self::config_in_payload(image)
  }

  pub(super) fn export_config(source: &Path, destination: &Path) -> Result<()> {
    let image = fs::read(source)?;
    if let Some(config) = Self::embedded_config(&image) {
      fs::write(destination, config)?;
      eprintln!("Exported kernel config to {}", destination.display());
    }
    Ok(())
  }

  fn config_in_payload(payload: &[u8]) -> Option<String> {
    const CONFIG_MAGIC: &[u8] = b"IKCFG_ST\x1f\x8b\x08";

    payload
      .windows(CONFIG_MAGIC.len())
      .position(|bytes| bytes == CONFIG_MAGIC)
      .and_then(|offset| payload.get(offset..))
      .and_then(|bytes| bytes.strip_prefix(b"IKCFG_ST"))
      .and_then(|compressed| {
        let mut config = String::new();
        if let Err(error) =
          GzDecoder::new(compressed).read_to_string(&mut config)
        {
          eprintln!("Embedded config decompression warning: {error}");
        }
        config.contains("CONFIG_").then_some(config)
      })
  }

  const fn x86_64(
    name: &'static str,
    smoke: bool,
    sha256: &'static str,
    source_sha256: &'static str,
  ) -> Self {
    Self {
      name,
      architecture: "x86_64",
      smoke,
      sha256: Some(sha256),
      source_sha256: Some(source_sha256),
    }
  }

  pub(super) async fn image(
    &self,
    cache: &Path,
    format: KernelImageFormat,
  ) -> Result<Option<PathBuf>> {
    match self.sha256 {
      | None => Ok(None),
      | Some(_) => {
        let path = self.built_image(cache, format);
        ensure!(
          path.is_file(),
          "built kernel missing: {}; run vmtest --kernel {} --build-kernel",
          path.display(),
          self.name
        );
        Ok(Some(path))
      },
    }
  }

  pub(super) fn built_image(
    &self,
    cache: &Path,
    format: KernelImageFormat,
  ) -> PathBuf {
    cache.join(format!("{}-v{}-boxlite", format.file_stem(), self.name))
  }

  pub(super) async fn fixture(&self, cache: &Path) -> Result<Option<PathBuf>> {
    let sha256 = match self.sha256 {
      | None => return Ok(None),
      | Some(sha256) => sha256,
    };
    let file_name = format!("bzImage-v{}-archlinux", self.name);
    let url = format!("https://github.com/danobi/vmtest/releases/download/test_assets/{file_name}");
    let destination = cache.join(file_name);
    match destination.try_exists()? && Self::matches(&destination, sha256)? {
      | true => return Ok(Some(destination)),
      | false => {},
    }

    fs::create_dir_all(cache)?;
    let temporary = destination.with_extension("download");
    let image = reqwest::get(&url)
      .await?
      .error_for_status()?
      .bytes()
      .await?;
    fs::write(&temporary, image)?;
    ensure!(
      Self::matches(&temporary, sha256)?,
      "checksum mismatch for {url}"
    );
    fs::rename(temporary, &destination)?;

    Ok(Some(destination))
  }

  fn matches(path: &Path, expected: &str) -> Result<bool> {
    Ok(hex::encode(Sha256::digest(fs::read(path)?)) == expected)
  }
}

#[cfg(test)]
mod tests {
  use std::io::Write;

  use flate2::{Compression, write::GzEncoder};

  use super::Kernel;

  #[test]
  fn extracts_config_from_plain_and_zstd_wrapped_images() {
    let config = "CONFIG_VIRTIO_BLK=m\nCONFIG_EXT4_FS=m\n";
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(config.as_bytes()).unwrap();
    let compressed_config = encoder.finish().unwrap();
    let payload =
      [b"kernel".as_slice(), b"IKCFG_ST", &compressed_config].concat();
    let compressed_payload =
      zstd::stream::encode_all(payload.as_slice(), 0).unwrap();
    let image = [b"boot header".as_slice(), &compressed_payload].concat();

    let elf = [b"\x7fELF".as_slice(), &payload].concat();
    for candidate in [payload, image, elf] {
      assert_eq!(Kernel::embedded_config(&candidate), Some(config.into()));
    }
  }
}
