//! Swap accounting and limits in the memory controller.

use std::str::FromStr;

#[cfg(target_os = "linux")]
use super::super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{KeyedFields, ParseCount},
  scalar::{Scalar, interface},
  unit::{Bytes, Count},
};

/// A borrowed view of one cgroup's swap interfaces.
///
/// Each method reads a fresh snapshot. Root cgroups and kernels without a
/// given interface return [`crate::Error::FileMissing`].
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Swap<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Swap<'_> {
  /// Current swap usage of this cgroup and its descendants, in bytes.
  pub fn current(&self) -> Result<SwapCurrent, Error> { self.cgroup.parse(SwapCurrent::FILE_NAME) }

  /// Peak swap usage since cgroup creation for this fresh descriptor.
  pub fn peak(&self) -> Result<SwapPeak, Error> { self.cgroup.parse(SwapPeak::FILE_NAME) }

  /// Hard swap limit configured for this cgroup.
  pub fn max(&self) -> Result<SwapMax, Error> { self.cgroup.parse(SwapMax::FILE_NAME) }

  /// Swap throttling limit configured for this cgroup.
  pub fn high(&self) -> Result<SwapHigh, Error> { self.cgroup.parse(SwapHigh::FILE_NAME) }

  /// Swap high, max, and allocation failure counters.
  pub fn events(&self) -> Result<SwapEvents, Error> { self.cgroup.parse(SwapEvents::FILE_NAME) }
}

/// Hierarchical swap usage reported by `memory.swap.current`.
pub type SwapCurrent = Scalar<Bytes, interface::SwapCurrent>;

/// Peak hierarchical swap usage reported by `memory.swap.peak`.
///
/// A write to an open file descriptor resets the peak for that descriptor.
/// This reader opens a fresh read-only descriptor, so its value is the peak
/// since cgroup creation. Older kernels may lack this file.
pub type SwapPeak = Scalar<Bytes, interface::SwapPeak>;

/// Hard swap limit reported by `memory.swap.max`.
///
/// `max` means no limit is configured here; ancestor limits can still apply.
pub type SwapMax = Scalar<MaxOr<Bytes>, interface::SwapMax>;

/// Swap throttling limit reported by `memory.swap.high`.
///
/// Exceeding this limit throttles further allocations. It is intended for
/// userspace out-of-memory handling rather than routine swap control.
pub type SwapHigh = Scalar<MaxOr<Bytes>, interface::SwapHigh>;

/// Swap event counters reported by `memory.swap.events`.
///
/// The `high` counter is absent on older kernels. Unknown future keys are
/// ignored; known counters must contain valid unsigned event counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwapEvents {
  high: Option<Count>,
  max: Count,
  fail: Count,
}

impl SwapEvents {
  /// The cgroup v2 `memory.swap.events` interface filename.
  pub const FILE_NAME: &'static str = "memory.swap.events";

  /// Times swap usage exceeded the high threshold, when reported.
  #[must_use]
  pub const fn high(self) -> Option<Count> { self.high }

  /// Times a swap allocation failed at the max boundary.
  #[must_use]
  pub const fn max(self) -> Count { self.max }

  /// Swap allocation failures from the limit or system-wide exhaustion.
  #[must_use]
  pub const fn fail(self) -> Count { self.fail }
}

impl FromStr for SwapEvents {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(contents, |key| match key {
      | "high" => "high",
      | "max" => "max",
      | "fail" => "fail",
      | _ => "value",
    })?;

    Ok(Self {
      high: fields.optional::<ParseCount, _>("high")?,
      max: fields.required::<ParseCount, _>("max")?,
      fail: fields.required::<ParseCount, _>("fail")?,
    })
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
  fn parses_swap_events() {
    Cases::<SwapEvents>::check([
      (
        indoc! {"
          high 2
          max 3
          fail 4
        "},
        Ok(SwapEvents {
          high: Some(Count { value: 2, ..Default::default() }),
          max: Count { value: 3, ..Default::default() },
          fail: Count { value: 4, ..Default::default() },
        }),
      ),
      (
        "future nope\nfail 0\nmax 1\n",
        Ok(SwapEvents {
          high: None,
          max: Count { value: 1, ..Default::default() },
          fail: Count { value: 0, ..Default::default() },
        }),
      ),
      ("", Err(Missing("max"))),
      ("max 1\n", Err(Missing("fail"))),
      ("max nope\nfail 0", Err(Invalid("max", "nope"))),
      ("max 1\nfail -1", Err(Invalid("fail", "-1"))),
      ("high nope\nmax 1\nfail 0", Err(Invalid("high", "nope"))),
      ("max 1 extra\nfail 0", Err(Excess)),
    ]);
  }
}
