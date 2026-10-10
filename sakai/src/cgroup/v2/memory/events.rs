use std::str::FromStr;

use crate::{
  error::{ParseError, ParseValueError},
  parse::{KeyedFields, ParseCount},
  unit::Count,
};

/// A point-in-time snapshot of hierarchical `memory.events` counters.
///
/// Counters include events in descendants and may change between reads.
/// A cgroup2 mount with `memory_localevents` reports local values instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryEvents {
  counts: MemoryEventCounts,
}

impl MemoryEvents {
  /// The cgroup v2 `memory.events` interface filename.
  pub const FILE_NAME: &'static str = "memory.events";

  /// Returns the event counts, measured as numbers of occurrences.
  #[must_use]
  pub const fn counts(self) -> MemoryEventCounts { self.counts }
}

impl FromStr for MemoryEvents {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> { Ok(Self { counts: contents.parse()? }) }
}

/// A point-in-time snapshot of `memory.events.local` counters.
///
/// These counters include events originating in this cgroup, excluding
/// descendants. Older kernels may lack this file altogether.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryEventsLocal {
  counts: MemoryEventCounts,
}

impl MemoryEventsLocal {
  /// The cgroup v2 `memory.events.local` interface filename.
  pub const FILE_NAME: &'static str = "memory.events.local";

  /// Returns the local event counts, measured as numbers of occurrences.
  #[must_use]
  pub const fn counts(self) -> MemoryEventCounts { self.counts }
}

impl FromStr for MemoryEventsLocal {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> { Ok(Self { counts: contents.parse()? }) }
}

/// Event counts shared by the hierarchical and local memory event files.
///
/// Missing optional counters mean the kernel does not report them; they do
/// not imply zero. Unknown future keys are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryEventCounts {
  low: Count,
  high: Count,
  max: Count,
  oom: Count,
  oom_kill: Count,
  oom_group_kill: Option<Count>,
  sock_throttled: Option<Count>,
}

impl MemoryEventCounts {
  /// Reclaims despite usage below the effective low boundary.
  #[must_use]
  pub const fn low(self) -> Count { self.low }

  /// Throttling and direct reclaim after crossing the high boundary.
  #[must_use]
  pub const fn high(self) -> Count { self.high }

  /// Attempts to cross the max boundary.
  #[must_use]
  pub const fn max(self) -> Count { self.max }

  /// Allocations about to fail at the memory limit.
  #[must_use]
  pub const fn oom(self) -> Count { self.oom }

  /// Processes killed by any OOM killer.
  #[must_use]
  pub const fn oom_kill(self) -> Count { self.oom_kill }

  /// Group OOM kills, when reported by the kernel.
  #[must_use]
  pub const fn oom_group_kill(self) -> Option<Count> { self.oom_group_kill }

  /// Network socket throttling events, when reported by the kernel.
  #[must_use]
  pub const fn sock_throttled(self) -> Option<Count> { self.sock_throttled }
}

impl FromStr for MemoryEventCounts {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(contents, |_| "value")?;

    Ok(Self {
      low: fields.required::<ParseCount, _>("low")?,
      high: fields.required::<ParseCount, _>("high")?,
      max: fields.required::<ParseCount, _>("max")?,
      oom: fields.required::<ParseCount, _>("oom")?,
      oom_kill: fields.required::<ParseCount, _>("oom_kill")?,
      oom_group_kill: fields.optional::<ParseCount, _>("oom_group_kill")?,
      sock_throttled: fields.optional::<ParseCount, _>("sock_throttled")?,
    })
  }
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;
  use indoc::indoc;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

  #[test]
  fn parses_hierarchical_and_local_event_snapshots() {
    let count = |value| Count { value, ..Default::default() };
    for (input, expected) in [
      (
        indoc! {"oom_kill 5\nfuture_counter nope\nhigh 2\nlow 1\noom_group_kill 6\nmax 3\noom 4\n\
        sock_throttled 7\n"},
        MemoryEventCounts {
          low: count(1),
          high: count(2),
          max: count(3),
          oom: count(4),
          oom_kill: count(5),
          oom_group_kill: Some(count(6)),
          sock_throttled: Some(count(7)),
        },
      ),
      ("low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\n", MemoryEventCounts {
        low: count(0),
        high: count(0),
        max: count(0),
        oom: count(0),
        oom_kill: count(0),
        oom_group_kill: None,
        sock_throttled: None,
      }),
    ] {
      assert_eq!(assert_ok!(input.parse::<MemoryEvents>()), MemoryEvents { counts: expected });
      assert_eq!(assert_ok!(input.parse::<MemoryEventsLocal>()), MemoryEventsLocal { counts: expected });
    }
  }

  #[test]
  fn rejects_malformed_event_snapshots() {
    let cases = [
      ("", Missing("low")),
      ("low 1\nhigh 2\nmax 3\noom 4", Missing("oom_kill")),
      ("low -1\nhigh 2\nmax 3\noom 4\noom_kill 5", Invalid("low", "-1")),
      ("low 1\nhigh 2\nmax 3\noom 4\noom_kill 5\noom_group_kill nope", Invalid("oom_group_kill", "nope")),
      (
        "low 1\nhigh 2\nmax 3\noom 4\noom_kill 5\nsock_throttled 18446744073709551616",
        Invalid("sock_throttled", "18446744073709551616"),
      ),
      ("low 1\nhigh", Missing("value")),
      ("low 1 2", Excess),
    ];
    Cases::<MemoryEvents>::check(cases.map(|(input, failure)| (input, Err(failure))));
    Cases::<MemoryEventsLocal>::check(cases.map(|(input, failure)| (input, Err(failure))));
  }
}
