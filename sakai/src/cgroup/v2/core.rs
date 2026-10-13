//! Core interfaces that explain controller availability and threaded topology.

pub use state::{CgroupEvents, CgroupStat, CgroupStatLocal};
pub use task::TaskId;

mod state;
mod task;

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;
use crate::{
  limit::MaxOr,
  scalar::{Scalar, interface},
  unit::Count,
};

/// This cgroup's own freeze request, encoded as zero or one.
///
/// This configuration snapshot excludes ancestor requests and does not indicate
/// whether freezing has completed. Read [`CgroupEvents::frozen`] for observed
/// state. The hierarchy root lacks this file.
/// See [Linux `cgroup.freeze`](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
pub type CgroupFreeze = Scalar<bool, interface::CgroupFreeze>;

/// Configured maximum descendant depth below this cgroup, in cgroup levels.
///
/// Zero prevents children; `max` removes this cgroup's depth restriction.
/// Ancestor restrictions still apply. Available at the hierarchy root too.
/// See [Linux `cgroup.max.depth`](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
pub type CgroupMaxDepth = Scalar<MaxOr<Count>, interface::CgroupMaxDepth>;

/// Configured maximum number of descendant cgroups, excluding this cgroup.
///
/// Zero prevents children; `max` removes this cgroup's count restriction.
/// Existing descendants may exceed a lowered limit. Ancestor restrictions
/// still apply. Available at the hierarchy root too.
/// See [Linux `cgroup.max.descendants`](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
pub type CgroupMaxDescendants = Scalar<MaxOr<Count>, interface::CgroupMaxDescendants>;

/// Whether pressure stall accounting is enabled for this cgroup.
///
/// This flag is local: disabling accounting here does not disable descendants'
/// accounting. Requires `CONFIG_PSI` and enabled kernel PSI support. The root
/// flag controls system-wide PSI accounting independently of its descendants.
/// Since Linux 6.1; see [Linux 6.1 source](https://github.com/torvalds/linux/blob/v6.1/kernel/cgroup/cgroup.c)
/// and [Linux PSI control](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
pub type CgroupPressure = Scalar<bool, interface::CgroupPressure>;

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
/// See [Linux `cgroup.type` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display)]
pub enum CgroupType {
  /// A normal, valid domain cgroup.
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  #[strum(serialize = "domain")]
  Domain,
  /// The resource domain at the root of a threaded subtree.
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  #[strum(serialize = "domain threaded")]
  DomainThreaded,
  /// A domain with invalid threaded topology; it cannot be populated.
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  #[strum(serialize = "domain invalid")]
  DomainInvalid,
  /// A member of a threaded subtree.
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  #[strum(serialize = "threaded")]
  Threaded,
}

impl CgroupType {
  /// The cgroup v2 `cgroup.type` interface filename.
  /// See [Linux `cgroup.type` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  pub const FILE_NAME: &'static str = "cgroup.type";
}

/// A controller name reported by `cgroup.controllers` or
/// `cgroup.subtree_control`.
///
/// The kernel may add controllers; [`Other`](Self::Other) retains names this
/// version of the library does not recognize.
/// See [Linux `cgroup.controllers` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Hash, strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[non_exhaustive]
pub enum CgroupController {
  /// CPU scheduling time and bandwidth (`cpu`).
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu).
  Cpu,
  /// CPU and NUMA memory-node placement (`cpuset`).
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset).
  Cpuset,
  /// Block-device I/O accounting and limits (`io`).
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#io).
  Io,
  /// Memory accounting, protection, and limits (`memory`).
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory).
  Memory,
  /// Explicit hugepage accounting and limits (`hugetlb`).
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#hugetlb).
  Hugetlb,
  /// Task-count limits, including threads (`pids`).
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
  Pids,
  /// RDMA resource accounting and limits (`rdma`).
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma).
  Rdma,
  /// Miscellaneous non-memory resource limits (`misc`).
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
  Misc,
  /// Device-memory accounting and limits (`dmem`).
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#dmem).
  ///
  /// Since Linux 6.14; see the [upstream Linux 6.14 source](https://github.com/torvalds/linux/blob/v6.14/Documentation/admin-guide/cgroup-v2.rst).
  Dmem,
  /// Cgroup-based performance-event monitoring (`perf_event`); see [perf_event_open(2)](https://man7.org/linux/man-pages/man2/perf_event_open.2.html).
  PerfEvent,
  /// An unrecognized controller name, retained verbatim for forward compatibility.
  /// See [Linux controller documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#controllers).
  #[strum(default, to_string = "{0}")]
  Other(String),
}

impl CgroupController {
  /// The cgroup v2 `cgroup.controllers` interface filename.
  /// See [Linux `cgroup.controllers` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  pub const CONTROLLERS_FILE_NAME: &'static str = "cgroup.controllers";
  /// The cgroup v2 `cgroup.subtree_control` interface filename.
  /// See [Linux `cgroup.subtree_control` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  pub const SUBTREE_CONTROL_FILE_NAME: &'static str = "cgroup.subtree_control";
}

/// A borrowed view of one open cgroup's core interfaces.
///
/// Each method reads a fresh snapshot; separate reads are not atomic together.
/// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Core<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Core<'_> {
  /// Reads process IDs in the reader's PID namespace, preserving order and duplicates.
  ///
  /// Contains this cgroup's processes, excluding descendants and zombies.
  /// A threaded domain reports the processes of its entire threaded subtree.
  /// IDs are volatile and may be recycled while reading; the list is not an
  /// atomic membership snapshot. Zero entries represent tasks invisible in
  /// the reader's PID namespace. See [`TaskId`].
  ///
  /// # Errors
  ///
  /// Reading from a threaded cgroup returns [`Error::NotSupported`]. See also
  /// the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn procs(&self) -> Result<Vec<TaskId>, Error> { self.task_ids(TaskId::PROCS_FILE_NAME) }

  /// Reads this cgroup's thread IDs in the reader's PID namespace.
  ///
  /// Excludes descendants. Preserves kernel order and possible duplicates;
  /// an empty list means no threads were reported. Zero entries represent
  /// tasks invisible in the reader's PID namespace. IDs can be
  /// recycled while reading. Available at the hierarchy root too. See [`TaskId`].
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn threads(&self) -> Result<Vec<TaskId>, Error> { self.task_ids(TaskId::THREADS_FILE_NAME) }

  /// Volatile populated and frozen lifecycle state; root cgroups lack this file.
  ///
  /// Reads a fresh [`CgroupEvents`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn events(&self) -> Result<CgroupEvents, Error> { self.cgroup.parse(CgroupEvents::FILE_NAME) }

  /// Volatile descendant and subsystem counts, including at the hierarchy root.
  ///
  /// Reads a fresh [`CgroupStat`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn stat(&self) -> Result<CgroupStat, Error> { self.cgroup.parse(CgroupStat::FILE_NAME) }

  /// Reads cumulative local freezing time, including ancestor-imposed freezing.
  ///
  /// Since Linux 6.18; non-root only. See [`CgroupStatLocal`].
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn stat_local(&self) -> Result<CgroupStatLocal, Error> { self.cgroup.parse(CgroupStatLocal::FILE_NAME) }

  /// Reads this cgroup's own freeze request, excluding ancestor requests.
  ///
  /// This reports configuration, not completed freezing. See [`CgroupFreeze`].
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn freeze(&self) -> Result<CgroupFreeze, Error> { self.cgroup.parse(CgroupFreeze::FILE_NAME) }

  /// Reads the configured descendant depth limit, including at the root.
  /// See [`CgroupMaxDepth`] for units and ancestor restrictions.
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max_depth(&self) -> Result<CgroupMaxDepth, Error> { self.cgroup.parse(CgroupMaxDepth::FILE_NAME) }

  /// Reads the configured descendant cgroup count limit, including at the root.
  /// See [`CgroupMaxDescendants`] for units and ancestor restrictions.
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max_descendants(&self) -> Result<CgroupMaxDescendants, Error> {
    self.cgroup.parse(CgroupMaxDescendants::FILE_NAME)
  }

  /// Reads the local pressure accounting enable flag, including at the root.
  ///
  /// Since Linux 6.1; see [`CgroupPressure`] for configuration requirements.
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn pressure_enabled(&self) -> Result<CgroupPressure, Error> { self.cgroup.parse(CgroupPressure::FILE_NAME) }

  /// Configuration snapshot; the hierarchy root may lack this file.
  ///
  /// Reads a fresh [`CgroupType`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn kind(&self) -> Result<CgroupType, Error> {
    let contents = self.cgroup.read(CgroupType::FILE_NAME)?;
    contents.trim().parse().map_err(|source| Error::Parse {
      path: self.cgroup.path().join(CgroupType::FILE_NAME),
      source: Box::new(CgroupTypeParseError { raw: contents, source }),
    })
  }

  /// Configuration snapshot of controllers available to enable for children.
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn controllers(&self) -> Result<Vec<CgroupController>, Error> {
    self.controller_list(CgroupController::CONTROLLERS_FILE_NAME)
  }

  /// Configuration snapshot of controllers enabled for children.
  /// See [Linux core cgroup interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn subtree_control(&self) -> Result<Vec<CgroupController>, Error> {
    self.controller_list(CgroupController::SUBTREE_CONTROL_FILE_NAME)
  }

  fn controller_list(&self, file: &'static str) -> Result<Vec<CgroupController>, Error> {
    Ok(self.cgroup.read(file)?.split_ascii_whitespace().map(CgroupController::from).collect())
  }

  fn task_ids(&self, file: &'static str) -> Result<Vec<TaskId>, Error> {
    TaskId::parse_list(&self.cgroup.read(file)?)
      .map_err(|source| Error::Parse { path: self.cgroup.path().join(file), source: Box::new(source) })
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::{CgroupController::*, *};

  #[test]
  fn parses_configuration_flags_and_descendant_limits() {
    for (raw, expected) in [("0\n", false), ("1\n", true)] {
      assert_eq!(assert_ok!(raw.parse::<CgroupFreeze>()), CgroupFreeze::new(expected));
      assert_eq!(assert_ok!(raw.parse::<CgroupPressure>()), CgroupPressure::new(expected));
    }
    for raw in ["2", "-1", "true", "1 0"] {
      assert_err!(raw.parse::<CgroupFreeze>());
      assert_err!(raw.parse::<CgroupPressure>());
    }
    for (raw, expected) in [("max", MaxOr::Max), ("0", MaxOr::Value(Count::default()))] {
      assert_eq!(assert_ok!(raw.parse::<CgroupMaxDepth>()), CgroupMaxDepth::new(expected));
      assert_eq!(assert_ok!(raw.parse::<CgroupMaxDescendants>()), CgroupMaxDescendants::new(expected));
    }
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn reads_configuration_and_task_lists_without_changing_files() {
    let cgroup = Cgroup::fixture("fixtures/p6_core_complete");
    let reader = cgroup.core();
    assert_eq!(assert_ok!(reader.procs()), [42, 0, 7, 42].map(|id| assert_ok!(TaskId::try_new(id))).to_vec());
    assert_eq!(assert_ok!(reader.threads()), [42, 43, 7].map(|id| assert_ok!(TaskId::try_new(id))).to_vec());
    assert_eq!(assert_ok!(reader.freeze()), CgroupFreeze::new(false));
    assert_eq!(assert_ok!(reader.pressure_enabled()), CgroupPressure::new(true));
    assert_eq!(assert_ok!(reader.max_depth()), CgroupMaxDepth::new(MaxOr::Max));
    assert_eq!(
      assert_ok!(reader.max_descendants()),
      CgroupMaxDescendants::new(MaxOr::Value(Count { value: 3, ..Default::default() }))
    );
    assert_eq!(assert_ok!(reader.stat_local()), assert_ok!("frozen_usec 123456".parse::<CgroupStatLocal>()));
    for (name, expected) in [
      (CgroupFreeze::FILE_NAME, "0\n"),
      (CgroupPressure::FILE_NAME, "1\n"),
      (CgroupMaxDepth::FILE_NAME, "max\n"),
      (CgroupMaxDescendants::FILE_NAME, "3\n"),
      (TaskId::PROCS_FILE_NAME, "42\n0\n7\n42\n"),
      (TaskId::THREADS_FILE_NAME, "42\n43\n7\n"),
      (CgroupStatLocal::FILE_NAME, "frozen_usec 123456\n"),
    ] {
      assert_eq!(assert_ok!(std::fs::read_to_string(cgroup.path().join(name))), expected);
    }
  }

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
