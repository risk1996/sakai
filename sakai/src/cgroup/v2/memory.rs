//! Memory controller interface files.

use std::str::FromStr;

pub use events::*;
pub use numa_stat::*;
pub use stat::*;
pub use swap::*;
pub use zswap::*;

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;
#[cfg(target_os = "linux")]
use crate::pressure::Pressure;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{ParseBoolean, ParseBytes, Parser},
  unit::Bytes,
};

mod events;
mod numa_stat;
mod stat;
mod swap;
mod zswap;

/// A borrowed view of one open cgroup's memory interfaces.
///
/// Each method reads a fresh snapshot through the cgroup's directory. The
/// hierarchy root and cgroups without the memory controller can lack these
/// files; such reads return [`Error::FileMissing`]. Separate reads are not
/// atomic together.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Memory<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Memory<'_> {
  /// Borrows this cgroup's swap interfaces.
  pub const fn swap(&self) -> Swap<'_> { Swap { cgroup: self.cgroup } }

  /// Borrows this cgroup's compressed swap interfaces.
  pub const fn zswap(&self) -> Zswap<'_> { Zswap { cgroup: self.cgroup } }

  /// Live memory usage of this cgroup and its descendants, in bytes.
  pub fn current(&self) -> Result<MemoryCurrent, Error> { self.cgroup.parse(MemoryCurrent::FILE_NAME) }

  /// Live peak usage, in bytes, since cgroup creation for this fresh descriptor.
  pub fn peak(&self) -> Result<MemoryPeak, Error> { self.cgroup.parse(MemoryPeak::FILE_NAME) }

  /// Live breakdown of memory usage, page quantities, and event counts.
  pub fn stat(&self) -> Result<MemoryStat, Error> { self.cgroup.parse(MemoryStat::FILE_NAME) }

  /// Live memory event counters for this cgroup and its descendants.
  pub fn events(&self) -> Result<MemoryEvents, Error> { self.cgroup.parse(MemoryEvents::FILE_NAME) }

  /// Live memory event counters originating in this cgroup only.
  pub fn events_local(&self) -> Result<MemoryEventsLocal, Error> { self.cgroup.parse(MemoryEventsLocal::FILE_NAME) }

  /// Live per-NUMA-node memory amounts, page quantities, and event counts.
  pub fn numa_stat(&self) -> Result<MemoryNumaStat, Error> { self.cgroup.parse(MemoryNumaStat::FILE_NAME) }

  /// Live PSI averages and totals. Never registers a pressure trigger.
  pub fn pressure(&self) -> Result<Pressure, Error> { self.cgroup.parse(Pressure::MEMORY_FILE_NAME) }

  /// Configuration snapshot of this cgroup's hard memory limit in bytes.
  pub fn max(&self) -> Result<MemoryMax, Error> { self.cgroup.parse(MemoryMax::FILE_NAME) }

  /// Configuration snapshot of this cgroup's throttling limit in bytes.
  pub fn high(&self) -> Result<MemoryHigh, Error> { self.cgroup.parse(MemoryHigh::FILE_NAME) }

  /// Configuration snapshot of this cgroup's best-effort memory protection.
  pub fn low(&self) -> Result<MemoryLow, Error> { self.cgroup.parse(MemoryLow::FILE_NAME) }

  /// Configuration snapshot of this cgroup's hard memory protection.
  pub fn min(&self) -> Result<MemoryMin, Error> { self.cgroup.parse(MemoryMin::FILE_NAME) }

  /// Configuration snapshot of group OOM kill behavior.
  pub fn oom_group(&self) -> Result<MemoryOomGroup, Error> { self.cgroup.parse(MemoryOomGroup::FILE_NAME) }
}

/// Whether this cgroup is treated as an indivisible workload by the OOM killer.
///
/// This is a point-in-time configuration reading. When enabled, a cgroup OOM
/// kills its tasks and descendant tasks together, except OOM-protected tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryOomGroup {
  value: bool,
}

impl MemoryOomGroup {
  /// The cgroup v2 `memory.oom.group` interface filename.
  pub const FILE_NAME: &'static str = "memory.oom.group";

  /// Returns whether group OOM killing is enabled.
  #[must_use]
  pub const fn value(self) -> bool { self.value }
}

impl FromStr for MemoryOomGroup {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParseBoolean, _>(contents, "oom.group")?;
    Ok(Self { value })
  }
}

/// Hierarchical memory usage reported by `memory.current`.
///
/// This is a point-in-time reading in bytes. It can temporarily exceed a
/// configured limit and should be read again for a fresh value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryCurrent {
  value: Bytes,
}

impl MemoryCurrent {
  /// The cgroup v2 `memory.current` interface filename.
  pub const FILE_NAME: &'static str = "memory.current";

  /// Returns the current hierarchical usage in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes { self.value }
}

impl FromStr for MemoryCurrent {
  type Err = ParseError<ParseValueError>;

  /// Parses one decimal byte count, rejecting missing or excess fields.
  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParseBytes, _>(contents, "current")?;
    Ok(Self { value })
  }
}

/// Peak hierarchical memory usage reported by `memory.peak`.
///
/// The kernel can reset the peak by writing to an open file descriptor. This
/// reader opens a fresh read-only descriptor for each call, so it never resets
/// the value and observes the peak since cgroup creation. Older kernels may
/// lack this file; the reader then returns [`Error::FileMissing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryPeak {
  value: Bytes,
}

impl MemoryPeak {
  /// The cgroup v2 `memory.peak` interface filename.
  pub const FILE_NAME: &'static str = "memory.peak";

  /// Returns the peak hierarchical usage in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes { self.value }
}

impl FromStr for MemoryPeak {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParseBytes, _>(contents, "peak")?;
    Ok(Self { value })
  }
}

/// The hard memory limit configured by `memory.max`.
///
/// `max` means no limit is configured here; ancestor limits can still apply.
/// This is a point-in-time configuration reading in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryMax {
  value: MaxOr<Bytes>,
}

impl MemoryMax {
  /// The cgroup v2 `memory.max` interface filename.
  pub const FILE_NAME: &'static str = "memory.max";

  /// Returns this cgroup's hard limit in bytes, or [`MaxOr::Max`].
  #[must_use]
  pub const fn value(self) -> MaxOr<Bytes> { self.value }
}

impl FromStr for MemoryMax {
  type Err = ParseError<ParseValueError>;

  /// Parses one decimal byte limit or the literal `max`.
  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParseBytes, _>(contents, "max")?;
    Ok(Self { value })
  }
}

/// The memory throttling limit configured by `memory.high`.
///
/// Crossing this boundary triggers reclaim and throttling, not an OOM kill.
/// `max` means no limit is configured here; ancestor limits can still apply.
/// This is a point-in-time configuration reading in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryHigh {
  value: MaxOr<Bytes>,
}

impl MemoryHigh {
  /// The cgroup v2 `memory.high` interface filename.
  pub const FILE_NAME: &'static str = "memory.high";

  /// Returns this cgroup's throttling limit in bytes, or [`MaxOr::Max`].
  #[must_use]
  pub const fn value(self) -> MaxOr<Bytes> { self.value }
}

impl FromStr for MemoryHigh {
  type Err = ParseError<ParseValueError>;

  /// Parses one decimal byte limit or the literal `max`.
  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParseBytes, _>(contents, "high")?;
    Ok(Self { value })
  }
}

/// The best-effort memory protection configured by `memory.low`.
///
/// Reclaim avoids memory below the effective low boundary while unprotected
/// memory is available. Ancestor settings can reduce the effective protection.
/// This is a point-in-time configuration reading in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryLow {
  value: Bytes,
}

impl MemoryLow {
  /// The cgroup v2 `memory.low` interface filename.
  pub const FILE_NAME: &'static str = "memory.low";

  /// Returns this cgroup's configured best-effort protection in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes { self.value }
}

impl FromStr for MemoryLow {
  type Err = ParseError<ParseValueError>;

  /// Parses one decimal byte count.
  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParseBytes, _>(contents, "low")?;
    Ok(Self { value })
  }
}

/// The hard memory protection configured by `memory.min`.
///
/// Memory below the effective min boundary cannot be reclaimed. If no
/// unprotected reclaimable memory remains, an OOM kill can follow. Ancestor
/// settings can reduce the effective protection. This is a point-in-time
/// configuration reading in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryMin {
  value: Bytes,
}

impl MemoryMin {
  /// The cgroup v2 `memory.min` interface filename.
  pub const FILE_NAME: &'static str = "memory.min";

  /// Returns this cgroup's configured hard protection in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes { self.value }
}

impl FromStr for MemoryMin {
  type Err = ParseError<ParseValueError>;

  /// Parses one decimal byte count.
  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParseBytes, _>(contents, "min")?;
    Ok(Self { value })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::parse::tests::Cases;

  #[test]
  fn parses_memory_current() { Cases::<MemoryCurrent>::bytes("current", |value| MemoryCurrent { value }); }

  #[test]
  fn parses_memory_peak() { Cases::<MemoryPeak>::bytes("peak", |value| MemoryPeak { value }); }

  #[test]
  fn parses_memory_limits() {
    Cases::<MemoryMax>::limit("max", |value| MemoryMax { value });
    Cases::<MemoryHigh>::limit("high", |value| MemoryHigh { value });
  }

  #[test]
  fn parses_memory_protections() {
    Cases::<MemoryLow>::bytes("low", |value| MemoryLow { value });
    Cases::<MemoryMin>::bytes("min", |value| MemoryMin { value });
  }

  #[test]
  fn parses_memory_oom_group() { Cases::<MemoryOomGroup>::boolean("oom.group", |value| MemoryOomGroup { value }); }
}
