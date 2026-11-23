use std::{
  fs::{self, File},
  io,
  path::{Path, PathBuf},
  process::Command,
};

use anyhow::{Context, Result, ensure};
use reqwest::Url;
use tempfile::NamedTempFile;

#[derive(Debug)]
pub(super) struct Kernel {
  pub(super) name: &'static str,
  pub(super) smoke: bool,
  sha256: &'static str,
}

impl Kernel {
  pub(super) const ALL: &[Self] = &[
    Self::new("5.15", true, "1150nwy48n0nvm4iab0xiy9qm7gwjnbdhkwh09wi7iz3qwhrz1r3"),
    Self::new("6.1", false, "0g3ap1x1yyyz54y2rnprs7vwbcpdak4ga501jgrrqjifv4890frh"),
    Self::new("6.6", false, "1pzyn022b9dqp64wxd1aq2jir5v30fnv36qkv8495m02zvzb2irv"),
    Self::new("6.12", false, "0f4789c3p6zliq57abw6c9n1kn18s69q9gl5fsxmy0xzqiscg2d3"),
    Self::new("6.18", true, "0l95x6ig47c0il70ki2lfzqhzn33j5kpjnaxcvsp6vjmz61mi862"),
  ];
  const ASSET_URL: &str = "https://github.com/danobi/vmtest/releases/download/test_assets";

  const fn new(name: &'static str, smoke: bool, sha256: &'static str) -> Self { Self { name, smoke, sha256 } }

  pub(super) fn filename(&self) -> String { format!("bzImage-v{}-archlinux", self.name) }

  pub(super) fn image(&self, cache: &Path) -> Result<PathBuf> {
    fs::create_dir_all(cache)?;
    let filename = self.filename();
    let path = cache.join(&filename);
    let upstream = format!("{}/{filename}", Self::ASSET_URL);
    let store_path = if path.try_exists()? {
      // CI can restore a staged image on a runner with an empty Nix store.
      let cached =
        Url::from_file_path(&path).map_err(|()| anyhow::anyhow!("invalid staging path: {}", path.display()))?;
      self.prefetch(&filename, cached.as_str()).or_else(|error| {
        eprintln!("Could not import cached Linux {} fixture: {error:#}", self.name);
        self.prefetch(&filename, &upstream)
      })?
    } else {
      self.prefetch(&filename, &upstream)?
    };
    // The container and CI cache need a real file outside /nix/store.
    let mut staged = NamedTempFile::new_in(cache)?;
    io::copy(&mut File::open(store_path)?, staged.as_file_mut())?;
    staged.persist(&path).with_context(|| format!("stage verified fixture at {}", path.display()))?;
    Ok(path)
  }

  fn prefetch(&self, filename: &str, source: &str) -> Result<PathBuf> {
    let output = Command::new("nix-prefetch-url")
      .args(["--type", "sha256", "--print-path", "--name", filename, source, self.sha256])
      .output()
      .context("run nix-prefetch-url for the kernel fixture")?;
    ensure!(
      output.status.success(),
      "could not fetch verified Linux {} fixture: {}",
      self.name,
      String::from_utf8_lossy(&output.stderr).trim_end()
    );
    let stdout = String::from_utf8(output.stdout).context("read Nix prefetch output")?;
    stdout.lines().nth(1).map(PathBuf::from).context("Nix did not report a kernel store path")
  }
}
