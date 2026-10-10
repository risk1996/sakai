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
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Pids<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Pids<'_> {
  /// Current hierarchical task count, which can exceed the configured maximum.
  pub fn current(&self) -> Result<PidsCurrent, Error> { self.cgroup.parse(PidsCurrent::FILE_NAME) }

  /// Configured task limit in counts, or `max` for no limit at this cgroup.
  pub fn max(&self) -> Result<PidsMax, Error> { self.cgroup.parse(PidsMax::FILE_NAME) }

  /// Hierarchical process-limit event counts.
  pub fn events(&self) -> Result<PidsEvents, Error> { self.cgroup.parse(PidsEvents::FILE_NAME) }
}

/// Current number of tasks in this cgroup and its descendants.
///
/// Measured in counts of kernel TIDs, including individual threads. This
/// volatile value can exceed `pids.max` after task migration or a limit
/// reduction; no relationship with a separately read limit is enforced.
pub type PidsCurrent = Scalar<Count, interface::PidsCurrent>;

/// Configured task limit reported by `pids.max`, measured in kernel TIDs.
///
/// This configuration snapshot can change between reads. Zero is a valid
/// limit. `max` means no limit here; ancestor limits can still apply.
pub type PidsMax = Scalar<MaxOr<Count>, interface::PidsMax>;

/// A point-in-time snapshot of hierarchical `pids.events` counters.
///
/// Counts include limit events in descendants and may change between reads.
/// The `pids_localevents` mount option, and older kernels before hierarchical
/// reporting, report local fork failures instead. Unknown future keys are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PidsEvents {
  max: Count,
}

impl PidsEvents {
  /// The cgroup v2 `pids.events` interface filename.
  pub const FILE_NAME: &'static str = "pids.events";

  /// Number of occurrences of the process limit being hit.
  #[must_use]
  pub const fn max(self) -> Count { self.max }
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
  use indoc::indoc;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

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
