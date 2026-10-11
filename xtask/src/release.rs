use std::{
  env,
  fs::{self, OpenOptions},
  io::Write,
  path::{Path, PathBuf},
  process::Command,
  time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};
use reqwest::{Client, StatusCode};
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Args)]
pub(crate) struct Release {
  #[command(subcommand)]
  command: ReleaseCommand,
}

#[derive(Debug, Subcommand)]
enum ReleaseCommand {
  /// Enable automatic version preparation only after the first stable tag.
  Baseline,
  /// Package, dry-run publishing, and record a credential-free candidate.
  Prepare {
    /// Require this stable tag to match HEAD and the shared package version.
    #[arg(long)]
    tag: Option<String>,
  },
  /// Repackage the tagged checkout and compare it with the downloaded candidate.
  Check {
    #[arg(long)]
    tag: String,
  },
  /// Report whether upload is needed; reject conflicting or yanked versions.
  RegistryStatus,
  /// Require the published registry checksum to match the candidate.
  VerifyRegistry,
  /// Create GitHub release notes only after the registry upload is verified.
  GithubRelease,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Metadata {
  workspace_root: PathBuf,
  target_directory: PathBuf,
  packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Package {
  name: String,
  version: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Candidate {
  version: String,
  tag: String,
  commit: String,
  archive: String,
  sha256: String,
}

#[derive(Debug, Deserialize)]
struct RegistryResponse {
  version: RegistryVersion,
}

#[derive(Debug, Deserialize)]
struct RegistryVersion {
  checksum: String,
  yanked: bool,
}

impl Release {
  const DIST: &str = "dist/release";
  const GITHUB_RELEASES: &str = "https://api.github.com/repos/risk1996/sakai/releases";
  const MANIFEST: &str = "dist/release/release.json";
  const NOTES: &str = "dist/release/release-notes.md";
  const REGISTRY_ATTEMPTS: usize = 12;
  const REGISTRY_RETRY_DELAY: Duration = Duration::from_secs(10);
  const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

  pub(crate) async fn run(&self) -> Result<()> {
    match &self.command {
      | ReleaseCommand::Baseline => {
        let tags = Self::command("git", &["tag", "--list", "v*"])?;
        let ready = tags.lines().any(|tag| Self::stable_version(tag).is_ok());
        Self::output("ready", &ready.to_string())
      },
      | ReleaseCommand::Prepare { tag } => Self::prepare(tag.as_deref()),
      | ReleaseCommand::Check { tag } => {
        let candidate = Self::candidate()?;
        let current = Self::package(Some(tag))?;
        ensure!(candidate == current, "repackaged archive differs from the checked candidate");
        Ok(())
      },
      | ReleaseCommand::RegistryStatus => {
        let needed = Self::registry_version(&Self::candidate()?).await?.is_none();
        Self::output("needs_publish", &needed.to_string())
      },
      | ReleaseCommand::VerifyRegistry => {
        let candidate = Self::candidate()?;
        for _ in 0..Self::REGISTRY_ATTEMPTS {
          match Self::registry_version(&candidate).await? {
            | Some(_) => return Ok(()),
            | None => tokio::time::sleep(Self::REGISTRY_RETRY_DELAY).await,
          }
        }
        bail!("published version did not appear in the crates.io API before the timeout")
      },
      | ReleaseCommand::GithubRelease => Self::github_release().await,
    }
  }

  fn stable_version(tag: &str) -> Result<Version> {
    let version = Version::parse(tag.strip_prefix('v').context("release tag must start with v")?)?;
    ensure!(version.pre.is_empty() && version.build.is_empty(), "only stable release versions are supported");
    ensure!(tag == format!("v{version}"), "release tag must use canonical SemVer");
    Ok(version)
  }

  fn command(program: &str, arguments: &[&str]) -> Result<String> {
    let output = Command::new(program).args(arguments).output().with_context(|| format!("run {program}"))?;
    ensure!(
      output.status.success(),
      "{program} failed: {}\n{}",
      String::from_utf8_lossy(&output.stdout),
      String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
  }

  fn output(name: &str, value: &str) -> Result<()> {
    println!("{name}={value}");
    match env::var_os("GITHUB_OUTPUT") {
      | Some(path) => {
        writeln!(OpenOptions::new().append(true).open(path)?, "{name}={value}").context("write action output")
      },
      | None => Ok(()),
    }
  }

  fn checksum(path: &Path) -> Result<String> {
    let path = path.to_str().context("archive path is not UTF-8")?;
    let digest = Self::command("sha256sum", &[path])?;
    let digest = digest.split_whitespace().next().context("missing SHA-256 checksum")?;
    ensure!(digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()), "invalid SHA-256 checksum");
    Ok(digest.to_owned())
  }

  fn metadata() -> Result<Metadata> {
    serde_json::from_str(&Self::command("cargo", &["metadata", "--no-deps", "--format-version", "1", "--locked"])?)
      .context("read workspace metadata")
  }

  fn package(tag: Option<&str>) -> Result<Candidate> {
    let metadata = Self::metadata()?;
    ensure!(env::current_dir()? == metadata.workspace_root, "run release tasks from the workspace root");
    ensure!(
      Self::command("git", &["status", "--porcelain"])?.trim().is_empty(),
      "release candidates require a clean Git checkout"
    );
    let package = metadata.packages.iter().find(|package| package.name == "sakai").context("missing sakai package")?;
    let tag_name = format!("v{}", package.version);
    Self::stable_version(&tag_name)?;
    let python =
      metadata.packages.iter().find(|package| package.name == "sakai-python").context("missing Python package")?;
    ensure!(package.version == python.version, "Rust and Python versions must stay synchronized");
    let commit = Self::command("git", &["rev-parse", "HEAD"])?.trim().to_owned();
    match tag {
      | Some(tag) => {
        Self::stable_version(tag)?;
        ensure!(tag == tag_name, "tag does not match the workspace version");
        let tagged_commit = Self::command("git", &["rev-parse", &format!("refs/tags/{tag}^{{commit}}")])?;
        ensure!(tagged_commit.trim() == commit, "checkout is not the tagged commit");
        Self::command("git", &["merge-base", "--is-ancestor", "HEAD", "origin/main"])?;
      },
      | None => {},
    }
    ensure!(
      Command::new("cargo").args(["publish", "--package", "sakai", "--dry-run", "--locked"]).status()?.success(),
      "Cargo publishing dry run failed"
    );
    let archive = format!("sakai-{}.crate", package.version);
    let sha256 = Self::checksum(&metadata.target_directory.join("package").join(&archive))?;
    Ok(Candidate { version: package.version.clone(), tag: tag_name, commit, archive, sha256 })
  }

  fn prepare(tag: Option<&str>) -> Result<()> {
    let candidate = Self::package(tag)?;
    let notes = Self::release_notes(&fs::read_to_string("sakai/CHANGELOG.md")?, &candidate.version)?;
    fs::create_dir_all(Self::DIST)?;
    fs::copy(
      Self::metadata()?.target_directory.join("package").join(&candidate.archive),
      Path::new(Self::DIST).join(&candidate.archive),
    )?;
    fs::write(Self::NOTES, notes)?;
    fs::write(Self::MANIFEST, serde_json::to_string_pretty(&candidate)?)?;
    Self::output("version", &candidate.version)?;
    Self::output("tag", &candidate.tag)?;
    Self::output("sha256", &candidate.sha256)
  }

  fn release_notes(changelog: &str, version: &str) -> Result<String> {
    let heading = format!("## [{version}]");
    let sections = changelog
      .lines()
      .skip_while(|line| !line.starts_with(&heading))
      .skip(1)
      .take_while(|line| !line.starts_with("## "))
      .collect::<Vec<_>>();
    let notes = sections.join("\n").trim().to_owned();
    ensure!(!notes.is_empty(), "missing nonempty changelog section for {version}");
    Ok(notes)
  }

  fn candidate() -> Result<Candidate> {
    let candidate: Candidate = serde_json::from_str(&fs::read_to_string(Self::MANIFEST)?)?;
    ensure!(Self::stable_version(&candidate.tag)?.to_string() == candidate.version, "candidate tag/version mismatch");
    ensure!(candidate.archive == format!("sakai-{}.crate", candidate.version), "unexpected candidate filename");
    ensure!(
      Self::checksum(&Path::new(Self::DIST).join(&candidate.archive))? == candidate.sha256,
      "candidate archive checksum mismatch"
    );
    Ok(candidate)
  }

  fn client() -> Result<Client> {
    Client::builder()
      .user_agent("sakai-release (https://github.com/risk1996/sakai)")
      .timeout(Self::REQUEST_TIMEOUT)
      .build()
      .context("build release HTTP client")
  }

  async fn registry_version(candidate: &Candidate) -> Result<Option<RegistryVersion>> {
    let response =
      Self::client()?.get(format!("https://crates.io/api/v1/crates/sakai/{}", candidate.version)).send().await?;
    match response.status() {
      | StatusCode::NOT_FOUND => Ok(None),
      | _ => {
        let published: RegistryResponse = serde_json::from_str(&response.error_for_status()?.text().await?)?;
        Self::matching_registry_version(candidate, &published.version)?;
        Ok(Some(published.version))
      },
    }
  }

  fn matching_registry_version(candidate: &Candidate, published: &RegistryVersion) -> Result<()> {
    ensure!(!published.yanked, "published version is yanked; do not reuse it");
    ensure!(
      published.checksum == candidate.sha256,
      "published version has a different archive; do not skip or overwrite it"
    );
    Ok(())
  }

  async fn github_release() -> Result<()> {
    let candidate = Self::candidate()?;
    ensure!(
      candidate.commit == Self::command("git", &["rev-parse", "HEAD"])?.trim(),
      "checkout differs from the release candidate's commit"
    );
    ensure!(Self::registry_version(&candidate).await?.is_some(), "crate upload has not completed");
    let notes = fs::read_to_string(Self::NOTES)?;
    ensure!(
      notes == Self::release_notes(&fs::read_to_string("sakai/CHANGELOG.md")?, &candidate.version)?,
      "candidate notes differ from the tagged changelog"
    );
    let token = env::var("GH_TOKEN").context("GitHub release job requires GH_TOKEN")?;
    let client = Self::client()?;
    let response =
      client.get(format!("{}/tags/{}", Self::GITHUB_RELEASES, candidate.tag)).bearer_auth(&token).send().await?;
    match response.status() {
      | StatusCode::NOT_FOUND => {
        let body = json!({ "tag_name": candidate.tag, "target_commitish": candidate.commit,
          "name": candidate.tag, "body": notes, "draft": false, "prerelease": false });
        client
          .post(Self::GITHUB_RELEASES)
          .bearer_auth(&token)
          .header("Content-Type", "application/json")
          .body(body.to_string())
          .send()
          .await?
          .error_for_status()?;
        println!("GitHub release created");
      },
      | _ => {
        let existing: serde_json::Value = serde_json::from_str(&response.error_for_status()?.text().await?)?;
        ensure!(
          existing.get("body").and_then(serde_json::Value::as_str) == Some(notes.as_str()),
          "existing GitHub release has different notes"
        );
        ensure!(
          existing.get("draft").and_then(serde_json::Value::as_bool) == Some(false),
          "existing GitHub release is still a draft"
        );
        ensure!(
          existing.get("prerelease").and_then(serde_json::Value::as_bool) == Some(false),
          "existing GitHub release is marked as a prerelease"
        );
        println!("Matching GitHub release already exists");
      },
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::{Candidate, RegistryVersion, Release};

  #[test]
  fn stable_tags() {
    for (tag, valid) in [
      ("v0.1.0", true),
      ("v1.2.3", true),
      ("0.1.0", false),
      ("v01.1.0", false),
      ("v0.1.0-rc.1", false),
      ("v0.1.0+build", false),
    ] {
      assert_eq!(Release::stable_version(tag).is_ok(), valid, "{tag}");
    }
  }

  #[test]
  fn changelog_sections() {
    let changelog = "# Changelog\n\n## [Unreleased]\n\nLater\n\n## [0.2.0](https://example.com) - 2026-10-11\n\n### \
                     Added\n\n- New\n\n## [0.1.0]\n\nInitial\n";
    for (version, notes) in [("0.2.0", Some("### Added\n\n- New")), ("0.1.0", Some("Initial")), ("0.3.0", None)] {
      assert_eq!(Release::release_notes(changelog, version).ok().as_deref(), notes);
    }
  }

  #[test]
  fn registry_retry_requires_identical_unyanked_archive() {
    let candidate = Candidate {
      version: "0.1.0".into(),
      tag: "v0.1.0".into(),
      commit: "commit".into(),
      archive: "sakai-0.1.0.crate".into(),
      sha256: "digest".into(),
    };
    for (checksum, yanked, valid) in [("digest", false, true), ("other", false, false), ("digest", true, false)] {
      let published = RegistryVersion { checksum: checksum.into(), yanked };
      assert_eq!(Release::matching_registry_version(&candidate, &published).is_ok(), valid);
    }
  }
}
