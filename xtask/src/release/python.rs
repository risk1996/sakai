use std::{
  collections::{BTreeMap, BTreeSet},
  fs::{self, File},
  path::Path,
  process::{Command, Stdio},
};

use anyhow::{Context, Result, bail, ensure};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use tar::{Archive, Builder};
use tempfile::{NamedTempFile, tempdir};

use super::Release;
use crate::http;

const SDIST_SUFFIX: &str = ".tar.gz";

#[derive(Deserialize)]
struct Lockfile {
  package: Vec<LockedPackage>,
}

#[derive(Deserialize)]
struct LockedPackage {
  #[serde(flatten)]
  identity: PackageIdentity,
  #[serde(default)]
  dependencies: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct PackageIdentity {
  name: String,
  version: String,
  source: Option<String>,
  checksum: Option<String>,
}

impl Lockfile {
  fn read(path: &Path) -> Result<Self> {
    toml::from_str(&fs::read_to_string(path)?).context("parse archived Cargo.lock")
  }

  /// Cargo omits an edge's version and source when the remaining fields identify it uniquely.
  fn dependency(&self, edge: &str) -> Result<&PackageIdentity> {
    let mut parts = edge.splitn(3, ' ');
    let name = parts.next().context("missing dependency name")?;
    let version = parts.next();
    let source = parts
      .next()
      .map(|source| {
        source.strip_prefix('(').and_then(|source| source.strip_suffix(')')).context("malformed dependency source")
      })
      .transpose()?;
    let mut matches = self.package.iter().map(|package| &package.identity).filter(|identity| {
      identity.name == name
        && version.is_none_or(|version| identity.version == version)
        && source.is_none_or(|source| identity.source.as_deref() == Some(source))
    });
    let matched = matches.next().with_context(|| format!("unresolved Cargo.lock dependency {edge:?}"))?;
    ensure!(matches.next().is_none(), "ambiguous Cargo.lock dependency {edge:?}");
    Ok(matched)
  }

  fn graph(&self) -> Result<BTreeMap<&PackageIdentity, BTreeSet<&PackageIdentity>>> {
    self
      .package
      .iter()
      .map(|package| {
        let dependencies = package.dependencies.iter().map(|edge| self.dependency(edge)).collect::<Result<_>>()?;
        Ok((&package.identity, dependencies))
      })
      .collect()
  }

  /// Allow pruning but reject a new identity or dependency edge.
  fn check_pruned(&self, pruned: &Self) -> Result<()> {
    let original = self.graph()?;
    for (package, dependencies) in pruned.graph()? {
      ensure!(
        original.get(package).is_some_and(|previous| dependencies.is_subset(previous)),
        "source distribution resolved a different dependency: {package:?}"
      );
    }
    Ok(())
  }
}

/// Prune the packaged workspace lockfile offline and replace just that archive entry.
// TODO: Remove this workaround once a Maturin release includes
// https://github.com/PyO3/maturin/pull/3192 and the unmodified Sakai source
// archive builds with `--locked --offline` from an extracted copy.
pub(super) fn prepare_sdist(path: &Path) -> Result<()> {
  let filename = path.file_name().and_then(|name| name.to_str()).context("missing UTF-8 source archive filename")?;
  let root = filename.strip_suffix(SDIST_SUFFIX).context("source archive must end in .tar.gz")?;
  let lock_entry = Path::new(root).join("Cargo.lock");
  let work = tempdir()?;
  let mut archive = Archive::new(GzDecoder::new(File::open(path)?));
  for entry in archive.entries()? {
    let mut entry = entry?;
    let kind = entry.header().entry_type();
    ensure!(kind.is_file() || kind.is_dir(), "unexpected entry type in source archive: {:?}", entry.path()?);
    ensure!(entry.unpack_in(work.path())?, "source archive entry escaped the staging directory");
  }

  let lockfile = work.path().join(&lock_entry);
  let original = Lockfile::read(&lockfile)?;
  let status = Command::new("cargo")
    .args(["metadata", "--format-version", "1", "--offline"])
    .current_dir(work.path().join(root))
    .stdout(Stdio::null())
    .status()
    .context("run Cargo metadata on extracted source distribution")?;
  ensure!(status.success(), "Cargo could not prune the archived lockfile");
  original.check_pruned(&Lockfile::read(&lockfile)?)?;

  let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
  let mut replacement = NamedTempFile::new_in(parent)?;
  let mut output = Builder::new(GzEncoder::new(replacement.as_file_mut(), Compression::default()));
  let mut archive = Archive::new(GzDecoder::new(File::open(path)?));
  for entry in archive.entries()?.raw(true) {
    let entry = entry?;
    let mut header = entry.header().clone();
    if header.path()? == lock_entry {
      let contents = fs::read(&lockfile)?;
      header.set_size(contents.len().try_into()?);
      header.set_cksum();
      output.append(&header, contents.as_slice())?;
    } else {
      output.append(&header, entry)?;
    }
  }
  output.into_inner()?.finish()?;
  replacement.persist(path).context("replace source archive with pruned lockfile")?;
  Ok(())
}

struct Distributions<'a> {
  directory: &'a Path,
  version: String,
  names: BTreeSet<String>,
}

#[derive(Deserialize)]
struct RegistryVersion {
  urls: Vec<RegistryFile>,
}

#[derive(Deserialize)]
struct RegistryFile {
  filename: String,
  yanked: bool,
  digests: RegistryDigests,
}

#[derive(Deserialize)]
struct RegistryDigests {
  sha256: String,
}

impl<'a> Distributions<'a> {
  fn read(directory: &'a Path) -> Result<Self> {
    let mut names = BTreeSet::new();
    for file in fs::read_dir(directory)? {
      let file = file?;
      ensure!(file.file_type()?.is_file(), "unexpected entry in PyPI candidate: {:?}", file.path());
      names.insert(file.file_name().into_string().ok().context("PyPI candidate has a non-UTF-8 filename")?);
    }
    let mut versions = names.iter().filter_map(|name| name.strip_prefix("sakai-")?.strip_suffix(SDIST_SUFFIX));
    let version = versions.next().context("missing PyPI source archive")?.to_owned();
    ensure!(versions.next().is_none(), "expected exactly one PyPI source archive");
    Ok(Self { directory, version, names })
  }

  fn registry_url(&self) -> Result<Url> {
    let mut url = Url::parse("https://pypi.org/pypi/sakai")?;
    url
      .path_segments_mut()
      .map_err(|()| anyhow::anyhow!("invalid PyPI registry base URL"))?
      .push(&self.version)
      .push("json");
    Ok(url)
  }

  /// Return missing filenames only after every existing upload matches the candidate.
  fn missing(&self, published: RegistryVersion) -> Result<BTreeSet<String>> {
    let mut missing = self.names.clone();
    for file in published.urls {
      let name = file.filename;
      ensure!(missing.remove(&name) && !file.yanked, "PyPI has an unexpected or yanked file: {name}");
      ensure!(
        Release::checksum(&self.directory.join(&name))? == file.digests.sha256,
        "PyPI checksum differs from the audited archive: {name}"
      );
    }
    Ok(missing)
  }

  async fn missing_at(&self, client: &Client, url: Url) -> Result<BTreeSet<String>> {
    let response = client.get(url).send().await?;
    if response.status() == StatusCode::NOT_FOUND {
      return Ok(self.names.clone());
    }
    self.missing(serde_json::from_str(&response.error_for_status()?.text().await?)?)
  }
}

pub(super) async fn stage(directory: &Path, output: &Path) -> Result<()> {
  let distributions = Distributions::read(directory)?;
  let missing = distributions.missing_at(&Release::client()?, distributions.registry_url()?).await?;
  fs::create_dir(output)?;
  for name in &missing {
    fs::copy(distributions.directory.join(name), output.join(name))?;
  }
  Release::output("needed", &(!missing.is_empty()).to_string())
}

pub(super) async fn verify(directory: &Path) -> Result<()> {
  let distributions = Distributions::read(directory)?;
  let client = Release::client()?;
  let url = distributions.registry_url()?;
  for attempt in 1..=Release::REGISTRY_ATTEMPTS {
    match distributions.missing_at(&client, url.clone()).await {
      | Ok(missing) if missing.is_empty() => return Ok(()),
      | Ok(missing) if attempt == Release::REGISTRY_ATTEMPTS => {
        bail!("PyPI did not return all audited files: {missing:?}")
      },
      | Ok(missing) => eprintln!("Waiting for PyPI files: {missing:?}"),
      | Err(error) if attempt < Release::REGISTRY_ATTEMPTS && http::retryable(&error) => {
        eprintln!("PyPI verification attempt {attempt} failed: {error:#}");
      },
      | Err(error) => return Err(error).with_context(|| format!("PyPI verification failed on attempt {attempt}")),
    }
    tokio::time::sleep(Release::REGISTRY_RETRY_DELAY).await;
  }
  bail!("PyPI verification exhausted its attempt budget")
}

#[cfg(test)]
mod tests {
  use std::{collections::BTreeSet, fs};

  use anyhow::{Result, ensure};
  use indoc::{formatdoc, indoc};
  use tempfile::tempdir;

  use super::{Distributions, Lockfile, RegistryDigests, RegistryFile, RegistryVersion, Release};

  #[test]
  fn pruning_keeps_the_resolved_identity_when_cargo_shortens_an_edge() -> Result<()> {
    let original: Lockfile = toml::from_str(indoc!(
      r#"
        [[package]]
        name = "binding"
        version = "1.0.0"
        dependencies = ["shared 1.0.0"]
        [[package]]
        name = "shared"
        version = "1.0.0"
        [[package]]
        name = "shared"
        version = "2.0.0"
      "#,
    ))?;
    for (version, valid) in [("1.0.0", true), ("2.0.0", false)] {
      let pruned = toml::from_str(&formatdoc!(
        r#"
          [[package]]
          name = "binding"
          version = "1.0.0"
          dependencies = ["shared"]
          [[package]]
          name = "shared"
          version = "{version}"
        "#,
      ))?;
      let result = original.check_pruned(&pruned);
      ensure!(result.is_ok() == valid, "unexpected pruning result for shared {version}: {result:?}");
    }
    Ok(())
  }

  #[test]
  fn partial_uploads_are_reusable_only_with_matching_unyanked_files() -> Result<()> {
    let directory = tempdir()?;
    let sdist = "sakai-0.1.0a2.tar.gz";
    let wheel = "sakai-0.1.0a2-cp311-abi3-example.whl";
    fs::write(directory.path().join(sdist), "source archive")?;
    fs::write(directory.path().join(wheel), "wheel")?;
    let distributions = Distributions::read(directory.path())?;
    let digest = Release::checksum(&directory.path().join(wheel))?;
    for (filename, yanked, sha256, valid) in [
      (wheel, false, digest.as_str(), true),
      (wheel, true, digest.as_str(), false),
      (wheel, false, "different", false),
      ("unexpected.whl", false, digest.as_str(), false),
    ] {
      let published = RegistryVersion {
        urls: vec![RegistryFile {
          filename: filename.into(),
          yanked,
          digests: RegistryDigests { sha256: sha256.into() },
        }],
      };
      let result = distributions.missing(published);
      if valid {
        ensure!(result? == BTreeSet::from([sdist.into()]), "only the source archive should be missing");
      } else {
        ensure!(result.is_err(), "registry accepted {filename} with yanked={yanked}, sha256={sha256}");
      }
    }
    Ok(())
  }
}
