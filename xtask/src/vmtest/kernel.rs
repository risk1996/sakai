use std::{
  fs,
  path::{Path, PathBuf},
};

use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};

pub(super) const KERNELS: &[Kernel] = &[
  Kernel {
    name: "native",
    architecture: "aarch64",
    smoke: true,
    sha256: None,
  },
  Kernel::x86_64(
    "5.15",
    true,
    "23879f21c7e3c7137902904fd89695fc9d8a938f1d2c1549dd1658443cb7a084",
  ),
  Kernel::x86_64(
    "6.1",
    false,
    "303b9010d92e4a9cf3930114f5c854edb2c5f7d1f9da2c3c29df7b1f7ab86a3c",
  ),
  Kernel::x86_64(
    "6.6",
    false,
    "3b47b1fefe02d49208da139bb1ad0363971ca5c02ab4ce89b9b8a52504b0fedf",
  ),
  Kernel::x86_64(
    "6.12",
    false,
    "a389c774c4bf035fbb7685be8493d128d8196c62862f750a8ef49b3b58428738",
  ),
  Kernel::x86_64(
    "6.18",
    true,
    "c2a05883f9556e73f5665d5979679163d80ff17754c4090e8d801df2a2e92551",
  ),
];

#[derive(Debug)]
pub(super) struct Kernel {
  pub(super) name: &'static str,
  pub(super) architecture: &'static str,
  pub(super) smoke: bool,
  pub(super) sha256: Option<&'static str>,
}

impl Kernel {
  const fn x86_64(
    name: &'static str,
    smoke: bool,
    sha256: &'static str,
  ) -> Self {
    Self {
      name,
      architecture: "x86_64",
      smoke,
      sha256: Some(sha256),
    }
  }

  pub(super) async fn image(&self, cache: &Path) -> Result<Option<PathBuf>> {
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
