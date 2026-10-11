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

/// Explicit release operations; package preparation never uploads a crate.
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

  /// Dispatch the selected operation, resolving credentials only for GitHub finalization.
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
        Self::verify_registry(
          &Self::client()?,
          &Self::registry_url(&candidate),
          &candidate,
          Self::REGISTRY_ATTEMPTS,
          Self::REGISTRY_RETRY_DELAY,
        )
        .await
      },
      | ReleaseCommand::GithubRelease => Self::github_release().await,
    }
  }

  /// Require a canonical stable version tag rather than a branch or prerelease.
  fn stable_version(tag: &str) -> Result<Version> {
    let version = Version::parse(tag.strip_prefix('v').context("release tag must start with v")?)?;
    ensure!(version.pre.is_empty() && version.build.is_empty(), "only stable release versions are supported");
    ensure!(tag == format!("v{version}"), "release tag must use canonical SemVer");
    Ok(version)
  }

  /// Capture a successful command's UTF-8 output, retaining failure diagnostics.
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

  /// Report a validated value locally and to GitHub Actions when available.
  fn output(name: &str, value: &str) -> Result<()> {
    println!("{name}={value}");
    match env::var_os("GITHUB_OUTPUT") {
      | Some(path) => {
        writeln!(OpenOptions::new().append(true).open(path)?, "{name}={value}").context("write action output")
      },
      | None => Ok(()),
    }
  }

  /// Compute and validate the archive's SHA-256 using devenv's coreutils.
  fn checksum(path: &Path) -> Result<String> {
    let path = path.to_str().context("archive path is not UTF-8")?;
    let digest = Self::command("sha256sum", &[path])?;
    let digest = digest.split_whitespace().next().context("missing SHA-256 checksum")?;
    ensure!(digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()), "invalid SHA-256 checksum");
    Ok(digest.to_owned())
  }

  /// Read locked Cargo metadata without resolving any registry credentials.
  fn metadata() -> Result<Metadata> {
    serde_json::from_str(&Self::command("cargo", &["metadata", "--no-deps", "--format-version", "1", "--locked"])?)
      .context("read workspace metadata")
  }

  /// Check source/version constraints and retain a verified, dry-run archive.
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
    // Publishing uses a temporary archive location. Produce the retained
    // candidate with Cargo's documented packaging command instead.
    ensure!(
      Command::new("cargo").args(["package", "--package", "sakai", "--locked"]).status()?.success(),
      "Cargo packaging failed"
    );
    let archive = format!("sakai-{}.crate", package.version);
    let sha256 = Self::checksum(&metadata.target_directory.join("package").join(&archive))?;
    Ok(Candidate { version: package.version.clone(), tag: tag_name, commit, archive, sha256 })
  }

  /// Save the candidate archive, source identity, checksum, and versioned notes.
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

  /// Extract one nonempty version section without including later release headings.
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

  /// Load the retained candidate and reject inconsistent identities or archive bytes.
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

  /// Build a bounded HTTP client with a public project contact and no credentials.
  fn client() -> Result<Client> {
    Client::builder()
      .user_agent("sakai-release (https://github.com/risk1996/sakai)")
      .timeout(Self::REQUEST_TIMEOUT)
      .build()
      .context("build release HTTP client")
  }

  /// Derive the public registry lookup URL from the validated candidate version.
  fn registry_url(candidate: &Candidate) -> String {
    format!("https://crates.io/api/v1/crates/sakai/{}", candidate.version)
  }

  /// Look up one registry version without interpreting API failures as absence.
  async fn registry_version(candidate: &Candidate) -> Result<Option<RegistryVersion>> {
    Self::registry_version_at(&Self::client()?, &Self::registry_url(candidate), candidate).await
  }

  /// Fetch and validate one response; only HTTP 404 means the version is missing.
  async fn registry_version_at(client: &Client, url: &str, candidate: &Candidate) -> Result<Option<RegistryVersion>> {
    let response = client.get(url).send().await?;
    match response.status() {
      | StatusCode::NOT_FOUND => Ok(None),
      | _ => {
        let published: RegistryResponse = serde_json::from_str(&response.error_for_status()?.text().await?)?;
        Self::matching_registry_version(candidate, &published.version)?;
        Ok(Some(published.version))
      },
    }
  }

  /// Wait within a fixed attempt budget for publication, retrying transient API failures.
  ///
  /// Authentication errors, malformed metadata, mismatched checksums, and yanked
  /// versions fail immediately. Failure diagnostics retain HTTP errors as causes.
  async fn verify_registry(
    client: &Client,
    url: &str,
    candidate: &Candidate,
    attempts: usize,
    retry_delay: Duration,
  ) -> Result<()> {
    ensure!(attempts > 0, "registry verification requires at least one attempt");
    for attempt in 1..=attempts {
      match Self::registry_version_at(client, url, candidate).await {
        | Ok(Some(_)) => return Ok(()),
        | Ok(None) if attempt < attempts => {
          eprintln!("registry verification attempt {attempt}/{attempts}: version not visible; retrying");
        },
        | Ok(None) => {
          bail!("published version did not appear in the crates.io API after {attempt}/{attempts} attempts")
        },
        | Err(error) if attempt < attempts && Self::retryable_registry_error(&error) => {
          eprintln!("registry verification attempt {attempt}/{attempts} failed: {error:#}; retrying");
        },
        | Err(error) => {
          return Err(error).with_context(|| format!("registry verification failed on attempt {attempt}/{attempts}"));
        },
      }
      tokio::time::sleep(retry_delay).await;
    }
    bail!("registry verification exhausted its attempt budget")
  }

  /// Retry transport failures, rate limiting, and server errors, not integrity failures.
  fn retryable_registry_error(error: &anyhow::Error) -> bool {
    match error.downcast_ref::<reqwest::Error>() {
      | Some(error) => match error.status() {
        | Some(status) => status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error(),
        | None => {
          error.is_connect() || error.is_request() || error.is_timeout() || error.is_body() || error.is_decode()
        },
      },
      | None => false,
    }
  }

  /// Require the existing immutable registry archive to be identical and unyanked.
  fn matching_registry_version(candidate: &Candidate, published: &RegistryVersion) -> Result<()> {
    ensure!(!published.yanked, "published version is yanked; do not reuse it");
    ensure!(
      published.checksum == candidate.sha256,
      "published version has a different archive; do not skip or overwrite it"
    );
    Ok(())
  }

  /// Create or validate non-draft GitHub release notes after registry verification.
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
  use std::{
    io::{BufRead, BufReader, ErrorKind, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
  };

  use anyhow::{Context, Result, ensure};
  use reqwest::Client;

  use super::{Candidate, RegistryVersion, Release};

  /// Canonical stable tags exclude ambiguous versions, branches, and prereleases.
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

  /// Versioned headings select only their own nonempty notes.
  #[test]
  fn changelog_sections() {
    let changelog = "# Changelog\n\n## [Unreleased]\n\nLater\n\n## [0.2.0](https://example.com) - 2026-10-11\n\n### \
                     Added\n\n- New\n\n## [0.1.0]\n\nInitial\n";
    for (version, notes) in [("0.2.0", Some("### Added\n\n- New")), ("0.1.0", Some("Initial")), ("0.3.0", None)] {
      assert_eq!(Release::release_notes(changelog, version).ok().as_deref(), notes);
    }
  }

  /// An existing upload is reusable only when its checksum matches and it is unyanked.
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

  /// Transient responses recover within the budget; integrity and client errors stop immediately.
  #[tokio::test]
  async fn verifies_registry_with_bounded_transient_retries() -> Result<()> {
    const MATCHING: &str = r#"{"version":{"checksum":"digest","yanked":false}}"#;
    const MISMATCHED: &str = r#"{"version":{"checksum":"other","yanked":false}}"#;
    const YANKED: &str = r#"{"version":{"checksum":"digest","yanked":true}}"#;
    for (responses, expected_error, expected_requests) in [
      (vec![("404 Not Found", ""), ("200 OK", MATCHING)], None, 2),
      (vec![("503 Service Unavailable", ""), ("200 OK", MATCHING)], None, 2),
      (vec![("429 Too Many Requests", ""), ("200 OK", MATCHING)], None, 2),
      (vec![("disconnect", ""), ("200 OK", MATCHING)], None, 2),
      (vec![("503 Service Unavailable", ""); 3], Some("attempt 3/3"), 3),
      (vec![("404 Not Found", ""); 3], Some("after 3/3 attempts"), 3),
      (vec![("401 Unauthorized", "")], Some("attempt 1/3"), 1),
      (vec![("403 Forbidden", "")], Some("attempt 1/3"), 1),
      (vec![("200 OK", MISMATCHED)], Some("different archive"), 1),
      (vec![("200 OK", YANKED)], Some("yanked"), 1),
      (vec![("200 OK", "invalid JSON")], Some("attempt 1/3"), 1),
    ] {
      let listener = TcpListener::bind("127.0.0.1:0")?;
      listener.set_nonblocking(true)?;
      let url = format!("http://{}/version", listener.local_addr()?);
      let server = thread::spawn(move || -> Result<usize> {
        for (status, body) in &responses {
          let started = Instant::now();
          let (mut stream, _) = loop {
            match listener.accept() {
              | Ok(connection) => break connection,
              | Err(error) if error.kind() == ErrorKind::WouldBlock && started.elapsed() < Duration::from_secs(5) => {
                thread::sleep(Duration::from_millis(1));
              },
              | Err(error) => return Err(error.into()),
            }
          };
          stream.set_nonblocking(false)?;
          stream.set_read_timeout(Some(Duration::from_secs(5)))?;
          let mut request = BufReader::new(&mut stream);
          loop {
            let mut line = String::new();
            ensure!(request.read_line(&mut line)? > 0, "incomplete HTTP request");
            if line == "\r\n" {
              break;
            }
          }
          if *status != "disconnect" {
            write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())?;
          }
        }
        Ok(responses.len())
      });
      let candidate = Candidate {
        version: "0.1.0".into(),
        tag: "v0.1.0".into(),
        commit: "commit".into(),
        archive: "sakai-0.1.0.crate".into(),
        sha256: "digest".into(),
      };
      let client = Client::builder().no_proxy().timeout(Duration::from_secs(5)).build()?;
      let result = Release::verify_registry(&client, &url, &candidate, 3, Duration::ZERO).await;
      let requests = server.join().map_err(|payload| anyhow::anyhow!("server panicked: {payload:?}"))??;
      ensure!(requests == expected_requests, "unexpected request count: {requests}");
      match expected_error {
        | Some(message) => ensure!(
          format!("{:#}", result.err().context("verification must fail")?).contains(message),
          "missing error diagnostic: {message}"
        ),
        | None => result?,
      }
    }
    Ok(())
  }

  /// Connection failures exhaust the retry budget without treating local errors as retryable.
  #[tokio::test]
  async fn retries_registry_connection_failures() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}/version", listener.local_addr()?);
    drop(listener);
    let client = Client::builder().no_proxy().timeout(Duration::from_secs(1)).build()?;
    let candidate = Candidate {
      version: "0.1.0".into(),
      tag: "v0.1.0".into(),
      commit: "commit".into(),
      archive: "sakai-0.1.0.crate".into(),
      sha256: "digest".into(),
    };
    let error = Release::verify_registry(&client, &url, &candidate, 3, Duration::ZERO)
      .await
      .err()
      .context("closed port must fail")?;
    ensure!(error.to_string().contains("attempt 3/3"), "retry budget not exhausted");
    ensure!(Release::retryable_registry_error(&error), "connection errors must be retryable");
    ensure!(
      !Release::retryable_registry_error(&std::io::Error::from(ErrorKind::PermissionDenied).into()),
      "local errors must not be retried"
    );
    Ok(())
  }

  /// A timed-out registry request is retryable rather than an integrity failure.
  #[tokio::test]
  async fn classifies_registry_timeout_as_transient() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let url = format!("http://{}/version", listener.local_addr()?);
    let server = thread::spawn(move || -> Result<()> {
      let started = Instant::now();
      let (stream, _) = loop {
        match listener.accept() {
          | Ok(connection) => break connection,
          | Err(error) if error.kind() == ErrorKind::WouldBlock && started.elapsed() < Duration::from_secs(5) => {
            thread::sleep(Duration::from_millis(1));
          },
          | Err(error) => return Err(error.into()),
        }
      };
      thread::sleep(Duration::from_millis(200));
      drop(stream);
      Ok(())
    });
    let client = Client::builder().no_proxy().timeout(Duration::from_millis(100)).build()?;
    let result = client.get(url).send().await;
    server.join().map_err(|payload| anyhow::anyhow!("server panicked: {payload:?}"))??;
    let error = result.err().context("unresponsive server must time out")?;
    ensure!(error.is_timeout(), "request failed for a reason other than timeout");
    ensure!(Release::retryable_registry_error(&error.into()), "timeouts must be retried");
    Ok(())
  }
}
