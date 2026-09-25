//! Core interfaces that explain controller availability and threaded topology.

use std::str::FromStr;

use crate::cgroup::common::error::Error;

/// The cgroup's domain/threaded topology state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString)]
pub enum CgroupType {
  #[strum(serialize = "domain")]
  Domain,
  #[strum(serialize = "domain threaded")]
  DomainThreaded,
  #[strum(serialize = "domain invalid")]
  DomainInvalid,
  #[strum(serialize = "threaded")]
  Threaded,
}

impl CgroupType {
  /// Parses a complete `cgroup.type` value, retaining invalid input in errors.
  pub fn parse(contents: &str) -> Result<Self, Error> {
    Self::from_str(contents.trim()).map_err(|error| Error::Parse {
      file: "cgroup.type",
      detail: format!("{contents:?}: {error}"),
    })
  }
}

/// Fresh configuration snapshots; separate calls are not atomic together.
pub trait ReadCore {
  /// Configuration snapshot; the hierarchy root may lack this file.
  fn ty(&self) -> Result<CgroupType, Error>;
  /// Configuration snapshot of controllers available to enable for children.
  fn controllers(&self) -> Result<Vec<String>, Error>;
  /// Configuration snapshot of controllers enabled for children.
  fn subtree_control(&self) -> Result<Vec<String>, Error>;
}

#[cfg(target_os = "linux")]
impl ReadCore for super::Cgroup {
  fn ty(&self) -> Result<CgroupType, Error> {
    CgroupType::parse(&self.read("cgroup.type")?)
  }

  fn controllers(&self) -> Result<Vec<String>, Error> {
    Ok(
      self
        .read("cgroup.controllers")?
        .split_ascii_whitespace()
        .map(str::to_owned)
        .collect(),
    )
  }

  fn subtree_control(&self) -> Result<Vec<String>, Error> {
    Ok(
      self
        .read("cgroup.subtree_control")?
        .split_ascii_whitespace()
        .map(str::to_owned)
        .collect(),
    )
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;

  #[test]
  fn parses_topology() {
    for (input, expected) in [
      ("domain\n", CgroupType::Domain),
      ("domain threaded\n", CgroupType::DomainThreaded),
      ("domain invalid\n", CgroupType::DomainInvalid),
      ("threaded\n", CgroupType::Threaded),
    ] {
      assert_eq!(assert_ok!(CgroupType::parse(input)), expected);
    }
    assert_err!(CgroupType::parse("unknown"));
  }
}
