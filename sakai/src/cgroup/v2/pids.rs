//! Process-number accounting and limits. Kernel PID counts include threads.

use crate::{
  limit::MaxOr,
  parse::{ParseCount, cgroup_value},
  scalar::{Scalar, interface},
  unit::Count,
};

/// Current number of tasks in this cgroup and its descendants.
///
/// Measured in counts of kernel TIDs, including individual threads. This
/// volatile value can exceed `pids.max` after task migration or a limit
/// reduction; no relationship with a separately read limit is enforced.
///
/// See [Linux `pids.current` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
pub type PidsCurrent = Scalar<Count, interface::PidsCurrent>;

/// Historical maximum task count for this cgroup and its descendants.
///
/// Counts kernel TIDs, including individual threads, since cgroup creation.
/// Includes transient charges later canceled by an ancestor-limit failure.
/// Reads do not reset this high-water mark. The hierarchy root lacks this file.
/// Since Linux 6.1; see [Linux 6.1 source](https://github.com/torvalds/linux/blob/v6.1/kernel/cgroup/pids.c)
/// and [Linux `pids.peak`](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
pub type PidsPeak = Scalar<Count, interface::PidsPeak>;

/// Configured task limit reported by `pids.max`, measured in kernel TIDs.
///
/// This configuration snapshot can change between reads. Zero is a valid
/// limit. `max` means no limit here; ancestor limits can still apply.
///
/// See [Linux `pids.max` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
pub type PidsMax = Scalar<MaxOr<Count>, interface::PidsMax>;

/// A point-in-time snapshot of `pids.events` or `pids.events.local` counters.
///
/// Since Linux 6.11, counts include failures caused by limits in this cgroup's
/// subtree. Before 6.11, this counter reports local fork failures, including
/// failures caused by an ancestor's limit. The `pids_localevents` mount option
/// restores the older semantics. Counts may change between reads; unknown
/// future keys are ignored. See the
/// [Linux 6.11 implementation](https://github.com/torvalds/linux/blob/v6.11/kernel/cgroup/pids.c).
/// `pids.events.local`, since Linux 6.11, normally counts failures at this
/// cgroup's own limit, including descendant requests; with `pids_localevents`,
/// both files instead count local fork failures, including ancestor-limit hits.
///
/// See [Linux `pids.events` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct PidsEvents {
  /// Number of occurrences of the process limit being hit.
  ///
  /// See [Linux pid interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
  pub max: Count,
}

crate::v2::interface::cgroup_file!(PidsEvents, "pids.events", Read);

impl PidsEvents {
  /// The cgroup v2 `pids.events` interface filename.
  ///
  /// See [Linux `pids.events` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;
  /// The non-root local event filename, available since Linux 6.11.
  ///
  /// See [Linux 6.11 source](https://github.com/torvalds/linux/blob/v6.11/kernel/cgroup/pids.c).
  pub const LOCAL_FILE_NAME: &'static str = "pids.events.local";
}

cgroup_value! { keyed PidsEvents, |_| "value", {
  max: required ParseCount => "max",
} }

crate::v2::interface::cgroup_file!(alias
  /// Local PID controller events.
  PidsEventsLocal, PidsEvents::LOCAL_FILE_NAME, Read, PidsEvents
);

#[cfg(test)]
mod tests {
  #[cfg(target_os = "linux")]
  use assertables::assert_ok;
  use indoc::indoc;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };
  #[cfg(target_os = "linux")]
  use crate::{Cgroup, Error};

  #[test]
  fn parses_lifetime_task_count_peaks_without_narrowing() {
    Cases::<PidsPeak>::check([
      ("0\n", Ok(PidsPeak::new(Count::default()))),
      ("18446744073709551615", Ok(PidsPeak::new(Count { value: u64::MAX, ..Default::default() }))),
      ("", Err(Missing("peak"))),
      ("max", Err(Invalid("peak", "max"))),
      ("-1", Err(Invalid("peak", "-1"))),
      ("1 2", Err(Excess)),
    ]);
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn reads_peaks_local_events_and_missing_optional_interfaces() {
    let cgroup = Cgroup::fixture("fixtures/p6_core_complete");
    assert_eq!(
      assert_ok!(cgroup.read_value::<crate::v2::pids::PidsPeak>()),
      PidsPeak::new(Count { value: 15, ..Default::default() })
    );
    assert_eq!(assert_ok!(cgroup.read_value::<crate::v2::pids::PidsEventsLocal>()), PidsEvents {
      max: Count { value: 2, ..Default::default() }
    });
    let cgroup = Cgroup::fixture("fixtures/p6_misc_older");
    for (file, result) in [
      (PidsPeak::FILE_NAME, cgroup.read_value::<crate::v2::pids::PidsPeak>().map(|_| ())),
      (PidsEvents::LOCAL_FILE_NAME, cgroup.read_value::<crate::v2::pids::PidsEventsLocal>().map(|_| ())),
    ] {
      assert!(matches!(result, Err(Error::FileMissing { path }) if path == cgroup.path().join(file)));
    }
  }

  #[test]
  fn parses_event_counts() {
    Cases::<PidsEvents>::check([
      ("max 0\n", Ok(PidsEvents { max: Count { value: 0, ..Default::default() } })),
      (
        indoc! {"
        future_counter nope
        max 18446744073709551615
      "},
        Ok(PidsEvents { max: Count { value: u64::MAX, ..Default::default() } }),
      ),
      ("", Err(Missing("max"))),
      ("future_counter 1", Err(Missing("max"))),
      ("max", Err(Missing("value"))),
      ("max -1", Err(Invalid("max", "-1"))),
      ("max nope", Err(Invalid("max", "nope"))),
      ("max 18446744073709551616", Err(Invalid("max", "18446744073709551616"))),
      ("max 1 extra", Err(Excess)),
    ]);
  }
}
