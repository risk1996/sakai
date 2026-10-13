//! Process-number accounting and limits. Kernel PID counts include threads.

use std::str::FromStr;

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{KeyedFields, ParseCount},
  scalar::{Scalar, interface},
  unit::Count,
};

/// A borrowed view of one cgroup's process-number controller.
///
/// Each read is a fresh snapshot; separate reads are not atomic together.
/// Missing interfaces, including disabled controllers and root exemptions,
/// return [`crate::Error::FileMissing`].
/// See [Linux pid interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Pids<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Pids<'_> {
  /// Current hierarchical task count, which can exceed the configured maximum.
  ///
  /// Reads a fresh [`PidsCurrent`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn current(&self) -> Result<PidsCurrent, Error> { self.cgroup.parse(PidsCurrent::FILE_NAME) }

  /// Reads the historical hierarchical task-count peak, including threads.
  ///
  /// Since Linux 6.1; see [`PidsPeak`]. Reading does not reset the peak.
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn peak(&self) -> Result<PidsPeak, Error> { self.cgroup.parse(PidsPeak::FILE_NAME) }

  /// Configured task limit in counts, or `max` for no limit at this cgroup.
  ///
  /// Reads a fresh [`PidsMax`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max(&self) -> Result<PidsMax, Error> { self.cgroup.parse(PidsMax::FILE_NAME) }

  /// Hierarchical process-limit event counts.
  ///
  /// Reads a fresh [`PidsEvents`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn events(&self) -> Result<PidsEvents, Error> { self.cgroup.parse(PidsEvents::FILE_NAME) }

  /// Reads local process-limit failures; available since Linux 6.11.
  ///
  /// Normally counts failures at this cgroup's own limit, including descendant
  /// requests. With `pids_localevents`, it counts fork failures originating in
  /// this cgroup, including ancestor-limit failures. See [`PidsEvents`].
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn events_local(&self) -> Result<PidsEvents, Error> { self.cgroup.parse(PidsEvents::LOCAL_FILE_NAME) }
}

/// Current number of tasks in this cgroup and its descendants.
///
/// Measured in counts of kernel TIDs, including individual threads. This
/// volatile value can exceed `pids.max` after task migration or a limit
/// reduction; no relationship with a separately read limit is enforced.
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
/// See [Linux `pids.events` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PidsEvents {
  max: Count,
}

impl PidsEvents {
  /// The cgroup v2 `pids.events` interface filename.
  /// See [Linux `pids.events` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
  pub const FILE_NAME: &'static str = "pids.events";
  /// The non-root local event filename, available since Linux 6.11.
  /// See [Linux 6.11 source](https://github.com/torvalds/linux/blob/v6.11/kernel/cgroup/pids.c).
  pub const LOCAL_FILE_NAME: &'static str = "pids.events.local";

  /// Number of occurrences of the process limit being hit.
  /// See [Linux pid interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#pid).
  #[must_use]
  pub const fn max(&self) -> Count { self.max }
}

impl FromStr for PidsEvents {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(contents, |_| "value")?;
    Ok(Self { max: fields.required::<ParseCount, _>("max")? })
  }
}

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
    assert_eq!(assert_ok!(cgroup.pids().peak()), PidsPeak::new(Count { value: 15, ..Default::default() }));
    assert_eq!(assert_ok!(cgroup.pids().events_local()), PidsEvents { max: Count { value: 2, ..Default::default() } });
    let cgroup = Cgroup::fixture("fixtures/p6_misc_older");
    for (file, result) in [
      (PidsPeak::FILE_NAME, cgroup.pids().peak().map(|_| ())),
      (PidsEvents::LOCAL_FILE_NAME, cgroup.pids().events_local().map(|_| ())),
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
