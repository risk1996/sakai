use std::{
  fs,
  io::{Read, Write},
  path::{Path, PathBuf},
  time::Duration,
};

use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use sha2::{Digest, Sha256};

pub(super) const KERNELS: &[Kernel] = &[
  Kernel::new(
    "5.15",
    true,
    "23879f21c7e3c7137902904fd89695fc9d8a938f1d2c1549dd1658443cb7a084",
  ),
  Kernel::new(
    "6.1",
    false,
    "303b9010d92e4a9cf3930114f5c854edb2c5f7d1f9da2c3c29df7b1f7ab86a3c",
  ),
  Kernel::new(
    "6.6",
    false,
    "3b47b1fefe02d49208da139bb1ad0363971ca5c02ab4ce89b9b8a52504b0fedf",
  ),
  Kernel::new(
    "6.12",
    false,
    "a389c774c4bf035fbb7685be8493d128d8196c62862f750a8ef49b3b58428738",
  ),
  Kernel::new(
    "6.18",
    true,
    "c2a05883f9556e73f5665d5979679163d80ff17754c4090e8d801df2a2e92551",
  ),
];

#[derive(Debug)]
pub(super) struct Kernel {
  pub(super) name: &'static str,
  pub(super) smoke: bool,
  pub(super) sha256: &'static str,
}

impl Kernel {
  const fn new(name: &'static str, smoke: bool, sha256: &'static str) -> Self {
    Self {
      name,
      smoke,
      sha256,
    }
  }

  pub(super) async fn image(&self, cache: &Path) -> Result<PathBuf> {
    fs::create_dir_all(cache)?;
    let file_name = format!("bzImage-v{}-archlinux", self.name);
    let destination = cache.join(&file_name);
    let lock = fs::File::create(cache.join(format!("{file_name}.lock")))?;
    lock.lock_exclusive()?;
    if destination.try_exists()? && Self::matches(&destination, self.sha256)? {
      return Ok(destination);
    }
    let url = format!("https://github.com/danobi/vmtest/releases/download/test_assets/{file_name}");
    let partial =
      cache.join(format!("{file_name}.{}.partial", std::process::id()));
    let result = async {
      let mut response = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?
        .get(&url)
        .send()
        .await?
        .error_for_status()?;
      let mut output = fs::File::create(&partial)?;
      while let Some(chunk) = response.chunk().await? {
        output.write_all(&chunk)?;
      }
      drop(output);
      ensure!(
        Self::matches(&partial, self.sha256)?,
        "checksum mismatch for {url}"
      );
      fs::rename(&partial, &destination)?;
      Ok::<_, anyhow::Error>(())
    }
    .await;
    if result.is_err() {
      drop(fs::remove_file(&partial));
    }
    result?;
    Ok(destination)
  }

  fn matches(path: &Path, expected: &str) -> Result<bool> {
    Ok(Self::digest(path)? == expected)
  }

  pub(super) fn digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
      match file.read(&mut buffer)? {
        | 0 => break,
        | size => hasher.update(
          buffer
            .get(..size)
            .context("read exceeded checksum buffer")?,
        ),
      }
    }
    Ok(hex::encode(hasher.finalize()))
  }
}
