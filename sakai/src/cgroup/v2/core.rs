//! Core interfaces that explain controller availability and threaded topology.

pub use state::{CgroupEvents, CgroupStat};

mod state;

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
///
/// See [Linux `cgroup.type` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display)]
pub enum CgroupType {
  /// A normal, valid domain cgroup.
  ///
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  #[strum(serialize = "domain")]
  Domain,
  /// The resource domain at the root of a threaded subtree.
  ///
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  #[strum(serialize = "domain threaded")]
  DomainThreaded,
  /// A domain with invalid threaded topology; it cannot be populated.
  ///
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  #[strum(serialize = "domain invalid")]
  DomainInvalid,
  /// A member of a threaded subtree.
  ///
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  #[strum(serialize = "threaded")]
  Threaded,
}

impl CgroupType {
  /// The cgroup v2 `cgroup.type` interface filename.
  ///
  /// See [Linux `cgroup.type` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  pub const FILE_NAME: &'static str = "cgroup.type";
}

/// A controller name reported by `cgroup.controllers` or
/// `cgroup.subtree_control`.
///
/// The kernel may add controllers; [`Other`](Self::Other) retains names this
/// version of the library does not recognize.
///
/// See [Linux `cgroup.controllers` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Hash, strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[non_exhaustive]
pub enum CgroupController {
  /// CPU scheduling time and bandwidth (`cpu`).
  ///
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu).
  Cpu,
  /// CPU and NUMA memory-node placement (`cpuset`).
  ///
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset).
  Cpuset,
  /// Block-device I/O accounting and limits (`io`).
  ///
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#io).
  Io,
  /// Memory accounting, protection, and limits (`memory`).
  ///
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory).
  Memory,
  /// Explicit hugepage accounting and limits (`hugetlb`).
  ///
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#hugetlb).
  Hugetlb,
  /// Task-count limits, including threads (`pids`).
  ///
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
  Pids,
  /// RDMA resource accounting and limits (`rdma`).
  ///
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma).
  Rdma,
  /// Miscellaneous non-memory resource limits (`misc`).
  ///
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
  Misc,
  /// Device-memory accounting and limits (`dmem`).
  ///
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#dmem).
  ///
  /// Since Linux 6.14; see the [upstream Linux 6.14 source](https://github.com/torvalds/linux/blob/v6.14/Documentation/admin-guide/cgroup-v2.rst).
  Dmem,
  /// Cgroup-based performance-event monitoring (`perf_event`); see [perf_event_open(2)](https://man7.org/linux/man-pages/man2/perf_event_open.2.html).
  PerfEvent,
  /// An unrecognized controller name, retained verbatim for forward compatibility.
  ///
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#controllers).
  #[strum(default, to_string = "{0}")]
  Other(String),
}

impl CgroupController {
  /// The cgroup v2 `cgroup.controllers` interface filename.
  ///
  /// See [Linux `cgroup.controllers` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  pub const CONTROLLERS_FILE_NAME: &'static str = "cgroup.controllers";
  /// The cgroup v2 `cgroup.subtree_control` interface filename.
  ///
  /// See [Linux `cgroup.subtree_control` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  pub const SUBTREE_CONTROL_FILE_NAME: &'static str = "cgroup.subtree_control";
}

/// A borrowed view of one open cgroup's core interfaces.
///
/// Each method reads a fresh snapshot; separate reads are not atomic together.
///
/// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Core<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Core<'_> {
  /// Volatile populated and frozen lifecycle state; root cgroups lack this file.
  ///
  /// Reads a fresh [`CgroupEvents`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn events(&self) -> Result<CgroupEvents, Error> { self.cgroup.parse(CgroupEvents::FILE_NAME) }

  /// Volatile descendant and subsystem counts, including at the hierarchy root.
  ///
  /// Reads a fresh [`CgroupStat`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn stat(&self) -> Result<CgroupStat, Error> { self.cgroup.parse(CgroupStat::FILE_NAME) }

  /// Configuration snapshot; the hierarchy root may lack this file.
  ///
  /// Reads a fresh [`CgroupType`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn kind(&self) -> Result<CgroupType, Error> {
    let contents = self.cgroup.read(CgroupType::FILE_NAME)?;
    contents.trim().parse().map_err(|source| Error::Parse {
      path: self.cgroup.path().join(CgroupType::FILE_NAME),
      source: Box::new(CgroupTypeParseError { raw: contents, source }),
    })
  }

  /// Configuration snapshot of controllers available to enable for children.
  ///
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  pub fn controllers(&self) -> Result<Vec<CgroupController>, Error> {
    self.controller_list(CgroupController::CONTROLLERS_FILE_NAME)
  }

  /// Configuration snapshot of controllers enabled for children.
  ///
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  pub fn subtree_control(&self) -> Result<Vec<CgroupController>, Error> {
    self.controller_list(CgroupController::SUBTREE_CONTROL_FILE_NAME)
  }

  fn controller_list(&self, file: &'static str) -> Result<Vec<CgroupController>, Error> {
    Ok(self.cgroup.read(file)?.split_ascii_whitespace().map(CgroupController::from).collect())
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::{CgroupController::*, *};

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
    let controllers = "cpu cpuset io memory hugetlb pids rdma misc dmem perf_event future_controller"
      .split_ascii_whitespace()
      .map(CgroupController::from)
      .collect::<Vec<_>>();
    assert_eq!(controllers, vec![
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
      Other("future_controller".into()),
    ]);
    assert_eq!(
      controllers.iter().map(ToString::to_string).collect::<Vec<_>>(),
      "cpu cpuset io memory hugetlb pids rdma misc dmem perf_event future_controller"
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
    );
    assert_eq!("\n".split_ascii_whitespace().map(CgroupController::from).collect::<Vec<_>>(), vec![]);
  }
}
