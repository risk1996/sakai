//! Core interfaces that explain controller availability and threaded topology.

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;

#[cfg(target_os = "linux")]
#[derive(Debug, thiserror::Error)]
#[error("cgroup content {raw:?} has an invalid cgroup type: {source}")]
struct CgroupTypeParseError {
  raw: String,
  #[source]
  source: strum::ParseError,
}

/// A cgroup's topology state, as read from `cgroup.type` on non-root cgroups.
///
/// Domain cgroups contain whole processes. A threaded domain is the resource
/// domain at the root of a threaded subtree; threaded cgroups beneath it can
/// contain individual threads. A newly created domain beneath a threaded
/// cgroup is invalid until converted to threaded. These four values are
/// defined for `cgroup.type` in the Linux kernel's
/// `Documentation/admin-guide/cgroup-v2.rst` (Core Interface Files).
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display,
)]
pub enum CgroupType {
  /// A normal, valid domain cgroup.
  #[strum(serialize = "domain")]
  Domain,
  /// The resource domain at the root of a threaded subtree.
  #[strum(serialize = "domain threaded")]
  DomainThreaded,
  /// A domain with invalid threaded topology; it cannot be populated.
  #[strum(serialize = "domain invalid")]
  DomainInvalid,
  /// A member of a threaded subtree.
  #[strum(serialize = "threaded")]
  Threaded,
}

/// A controller name reported by `cgroup.controllers` or
/// `cgroup.subtree_control`.
///
/// The kernel may add controllers; [`Other`](Self::Other) retains names this
/// version of the library does not recognize.
#[derive(
  Debug, Clone, PartialEq, Eq, Hash, strum::EnumString, strum::Display,
)]
#[strum(serialize_all = "snake_case")]
pub enum CgroupController {
  Cpu,
  Cpuset,
  Io,
  Memory,
  Hugetlb,
  Pids,
  Rdma,
  Misc,
  Dmem,
  PerfEvent,
  #[strum(default, to_string = "{0}")]
  Other(String),
}

/// A borrowed view of one open cgroup's core interfaces.
///
/// Each method reads a fresh snapshot; separate reads are not atomic together.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Core<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Core<'_> {
  /// Configuration snapshot; the hierarchy root may lack this file.
  pub fn kind(&self) -> Result<CgroupType, Error> {
    let contents = self.cgroup.read("cgroup.type")?;
    contents.trim().parse().map_err(|source| Error::Parse {
      path: self.cgroup.path().join("cgroup.type"),
      source: Box::new(CgroupTypeParseError {
        raw: contents,
        source,
      }),
    })
  }

  /// Configuration snapshot of controllers available to enable for children.
  pub fn controllers(&self) -> Result<Vec<CgroupController>, Error> {
    Ok(
      self
        .cgroup
        .read("cgroup.controllers")?
        .split_ascii_whitespace()
        .map(CgroupController::from)
        .collect(),
    )
  }

  /// Configuration snapshot of controllers enabled for children.
  pub fn subtree_control(&self) -> Result<Vec<CgroupController>, Error> {
    Ok(
      self
        .cgroup
        .read("cgroup.subtree_control")?
        .split_ascii_whitespace()
        .map(CgroupController::from)
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
      assert_eq!(assert_ok!(input.trim().parse::<CgroupType>()), expected);
      assert_eq!(expected.to_string(), input.trim());
    }
    assert_err!("unknown".parse::<CgroupType>());
  }

  #[test]
  fn parses_controllers() {
    let controllers = "cpu cpuset io memory hugetlb pids rdma misc dmem \
                       perf_event future_controller"
      .split_ascii_whitespace()
      .map(CgroupController::from)
      .collect::<Vec<_>>();
    assert_eq!(controllers, vec![
      CgroupController::Cpu,
      CgroupController::Cpuset,
      CgroupController::Io,
      CgroupController::Memory,
      CgroupController::Hugetlb,
      CgroupController::Pids,
      CgroupController::Rdma,
      CgroupController::Misc,
      CgroupController::Dmem,
      CgroupController::PerfEvent,
      CgroupController::Other("future_controller".into()),
    ]);
    assert_eq!(
      controllers
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>(),
      "cpu cpuset io memory hugetlb pids rdma misc dmem perf_event \
       future_controller"
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
    );
    assert_eq!(
      "\n"
        .split_ascii_whitespace()
        .map(CgroupController::from)
        .collect::<Vec<_>>(),
      vec![]
    );
  }
}
