//! Memory controller interface files. Reads return fresh, separate snapshots.
//! Scalar memory amounts are bytes; limits and protections describe this cgroup's settings.

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
  limit::MaxOr,
  scalar::{Scalar, interface},
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
/// files; such reads return [`crate::Error::FileMissing`]. Separate reads are not
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
/// When enabled, a cgroup OOM kills its tasks and descendant tasks together,
/// except OOM-protected tasks.
pub type MemoryOomGroup = Scalar<bool, interface::MemoryOomGroup>;

/// Hierarchical memory usage reported by `memory.current`.
///
/// Usage is in bytes and can temporarily exceed configured limits.
pub type MemoryCurrent = Scalar<Bytes, interface::MemoryCurrent>;

/// Peak hierarchical memory usage reported by `memory.peak`.
///
/// The kernel can reset the peak by writing to an open file descriptor. This
/// reader opens a fresh read-only descriptor for each call, so it never resets
/// the value and observes the peak since cgroup creation. Older kernels may
/// lack this file; the reader then returns [`crate::Error::FileMissing`].
pub type MemoryPeak = Scalar<Bytes, interface::MemoryPeak>;

/// The hard memory limit configured by `memory.max`.
///
/// `max` means no limit is configured here; ancestor limits can still apply.
pub type MemoryMax = Scalar<MaxOr<Bytes>, interface::MemoryMax>;

/// The memory throttling limit configured by `memory.high`.
///
/// Crossing this boundary triggers reclaim and throttling, not an OOM kill.
/// `max` means no limit is configured here; ancestor limits can still apply.
pub type MemoryHigh = Scalar<MaxOr<Bytes>, interface::MemoryHigh>;

/// The best-effort memory protection configured by `memory.low`.
///
/// Reclaim avoids memory below the effective low boundary while unprotected
/// memory is available. Ancestor settings can reduce the effective protection.
pub type MemoryLow = Scalar<Bytes, interface::MemoryLow>;

/// The hard memory protection configured by `memory.min`.
///
/// Memory below the effective min boundary cannot be reclaimed. If no
/// unprotected reclaimable memory remains, an OOM kill can follow. Ancestor
/// settings can reduce the effective protection.
pub type MemoryMin = Scalar<Bytes, interface::MemoryMin>;
