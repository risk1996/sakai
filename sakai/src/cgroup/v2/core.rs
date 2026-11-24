//! Core interfaces that explain controller availability and threaded topology.

pub use state::{CgroupEvents, CgroupStat, CgroupStatLocal};
pub use task::TaskId;

mod state;
mod task;

use super::interface::cgroup_file;
use crate::{
  limit::MaxOr,
  parse::cgroup_value,
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

#[derive(Debug, thiserror::Error)]
#[error("cgroup content {raw:?} has an invalid cgroup type: {source}")]
struct CgroupTypeParseError {
  raw: String,
  #[source]
  source: strum::ParseError,
}

/// A cgroup's topology state, as read from `cgroup.type` on non-root cgroups.
/// The typed write command `()` requests conversion to a threaded cgroup.
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
#[non_exhaustive]
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

cgroup_value! { display CgroupType }

crate::v2::interface::cgroup_file!(file CgroupType, "cgroup.type", ReadWrite);

impl CgroupType {
  /// The cgroup v2 `cgroup.type` interface filename.
  ///
  /// See [Linux `cgroup.type` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;
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

/// One controller change to submit to `cgroup.subtree_control`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubtreeControl {
  /// Enables this controller for child cgroups.
  Enable(CgroupController),
  /// Disables this controller for child cgroups.
  Disable(CgroupController),
}

/// A task to migrate, interpreted in the writer's PID namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationTarget {
  /// A process or thread ID. Zero is rejected; use [`Self::Caller`] explicitly.
  Id(TaskId),
  /// The process or thread issuing the write.
  Caller,
}

impl MigrationTarget {
  fn command(self) -> Result<String, crate::error::EncodeError> {
    match self {
      | Self::Caller => Ok("0".to_owned()),
      | Self::Id(id) if *id.as_ref() > 0 => id.encode_cgroup(),
      | Self::Id(_) => Err(crate::error::EncodeError { field: "task_id" }),
    }
  }
}

cgroup_file!(read CgroupType => CgroupType, |contents: &str| {
  Ok(contents.trim().parse().map_err(|source| CgroupTypeParseError { raw: contents.into(), source })?)
});
cgroup_file!(write CgroupType, 'a => (), |()| CgroupType::Threaded.encode_cgroup());

cgroup_file!(alias
  /// The cgroup's available controllers.
  CgroupControllers, CgroupController::CONTROLLERS_FILE_NAME, Read, Vec<CgroupController>, decode_controllers
);
cgroup_file!(alias
  /// Controllers enabled for children and their enable or disable actions.
  CgroupSubtreeControl, CgroupController::SUBTREE_CONTROL_FILE_NAME, ReadWrite, Vec<CgroupController>, decode_controllers
);
cgroup_file!(write CgroupSubtreeControl, 'a => &'a [SubtreeControl], encode_subtree_control);

fn decode_controllers(contents: &str) -> Result<Vec<CgroupController>, Box<dyn std::error::Error + Send + Sync>> {
  Ok(contents.split_ascii_whitespace().map(CgroupController::from).collect())
}

fn encode_subtree_control(actions: &[SubtreeControl]) -> Result<String, crate::error::EncodeError> {
  let mut command = String::new();
  for action in actions {
    let (prefix, controller) = match action {
      | SubtreeControl::Enable(controller) => ('+', controller),
      | SubtreeControl::Disable(controller) => ('-', controller),
    };
    let name = controller.to_string();
    super::interface::check_token(&name, "controller")?;
    if !command.is_empty() {
      command.push(' ');
    }
    command.push(prefix);
    command.push_str(&name);
  }
  if command.is_empty() {
    return Err(crate::error::EncodeError { field: "controllers" });
  }
  Ok(command)
}

cgroup_file!(alias
  /// Process membership and migration.
  CgroupProcs, TaskId::PROCS_FILE_NAME, ReadWrite, Vec<TaskId>, decode_tasks
);
cgroup_file!(alias
  /// Thread membership and migration.
  CgroupThreads, TaskId::THREADS_FILE_NAME, ReadWrite, Vec<TaskId>, decode_tasks
);
cgroup_file!(write CgroupProcs, 'a => MigrationTarget, MigrationTarget::command);
cgroup_file!(write CgroupThreads, 'a => MigrationTarget, MigrationTarget::command);

fn decode_tasks(contents: &str) -> Result<Vec<TaskId>, Box<dyn std::error::Error + Send + Sync>> {
  Ok(TaskId::parse_list(contents)?)
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::{CgroupController::*, *};
  #[cfg(target_os = "linux")]
  use crate::Cgroup;

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
    assert_eq!(
      assert_ok!(cgroup.read_value::<CgroupProcs>()),
      [42, 0, 7, 42].map(|id| assert_ok!(TaskId::try_new(id))).to_vec()
    );
    assert_eq!(
      assert_ok!(cgroup.read_value::<CgroupThreads>()),
      [42, 43, 7].map(|id| assert_ok!(TaskId::try_new(id))).to_vec()
    );
    assert_eq!(assert_ok!(cgroup.read_value::<CgroupFreeze>()), CgroupFreeze::new(false));
    assert_eq!(assert_ok!(cgroup.read_value::<CgroupPressure>()), CgroupPressure::new(true));
    assert_eq!(assert_ok!(cgroup.read_value::<CgroupMaxDepth>()), CgroupMaxDepth::new(MaxOr::Max));
    assert_eq!(
      assert_ok!(cgroup.read_value::<CgroupMaxDescendants>()),
      CgroupMaxDescendants::new(MaxOr::Value(Count { value: 3, ..Default::default() }))
    );
    assert_eq!(
      assert_ok!(cgroup.read_value::<CgroupStatLocal>()),
      assert_ok!("frozen_usec 123456".parse::<CgroupStatLocal>())
    );
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
