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
///
/// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Memory<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Memory<'_> {
  /// Borrows this cgroup's swap interfaces.
  ///
  /// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  pub const fn swap(&self) -> Swap<'_> { Swap { cgroup: self.cgroup } }

  /// Borrows this cgroup's compressed swap interfaces.
  ///
  /// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  pub const fn zswap(&self) -> Zswap<'_> { Zswap { cgroup: self.cgroup } }

  /// Live memory usage of this cgroup and its descendants, in bytes.
  ///
  /// Reads a fresh [`MemoryCurrent`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn current(&self) -> Result<MemoryCurrent, Error> { self.cgroup.parse(MemoryCurrent::FILE_NAME) }

  /// Live peak usage, in bytes, since cgroup creation for this fresh descriptor.
  ///
  /// Reads a fresh [`MemoryPeak`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/Documentation/admin-guide/cgroup-v2.rst).
  /// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn peak(&self) -> Result<MemoryPeak, Error> { self.cgroup.parse(MemoryPeak::FILE_NAME) }

  /// Live breakdown of memory usage, page quantities, and event counts.
  ///
  /// Reads a fresh [`MemoryStat`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn stat(&self) -> Result<MemoryStat, Error> { self.cgroup.parse(MemoryStat::FILE_NAME) }

  /// Live memory event counters for this cgroup and its descendants.
  ///
  /// Reads a fresh [`MemoryEvents`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn events(&self) -> Result<MemoryEvents, Error> { self.cgroup.parse(MemoryEvents::FILE_NAME) }

  /// Live memory event counters originating in this cgroup only.
  ///
  /// Reads a fresh [`MemoryEventsLocal`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn events_local(&self) -> Result<MemoryEventsLocal, Error> { self.cgroup.parse(MemoryEventsLocal::FILE_NAME) }

  /// Live per-NUMA-node memory amounts, page quantities, and event counts.
  ///
  /// Reads a fresh [`MemoryNumaStat`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn numa_stat(&self) -> Result<MemoryNumaStat, Error> { self.cgroup.parse(MemoryNumaStat::FILE_NAME) }

  /// Live PSI averages and totals. Never registers a pressure trigger.
  ///
  /// Reads a fresh [`Pressure`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn pressure(&self) -> Result<Pressure, Error> { self.cgroup.parse(Pressure::MEMORY_FILE_NAME) }

  /// Configuration snapshot of this cgroup's hard memory limit in bytes.
  ///
  /// Reads a fresh [`MemoryMax`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn max(&self) -> Result<MemoryMax, Error> { self.cgroup.parse(MemoryMax::FILE_NAME) }

  /// Configuration snapshot of this cgroup's throttling limit in bytes.
  ///
  /// Reads a fresh [`MemoryHigh`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn high(&self) -> Result<MemoryHigh, Error> { self.cgroup.parse(MemoryHigh::FILE_NAME) }

  /// Configuration snapshot of this cgroup's best-effort memory protection.
  ///
  /// Reads a fresh [`MemoryLow`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn low(&self) -> Result<MemoryLow, Error> { self.cgroup.parse(MemoryLow::FILE_NAME) }

  /// Configuration snapshot of this cgroup's hard memory protection.
  ///
  /// Reads a fresh [`MemoryMin`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn min(&self) -> Result<MemoryMin, Error> { self.cgroup.parse(MemoryMin::FILE_NAME) }

  /// Configuration snapshot of group OOM kill behavior.
  ///
  /// Reads a fresh [`MemoryOomGroup`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Io`]
  /// if the read fails for another reason.
  pub fn oom_group(&self) -> Result<MemoryOomGroup, Error> { self.cgroup.parse(MemoryOomGroup::FILE_NAME) }
}

/// Whether this cgroup is treated as an indivisible workload by the OOM killer.
///
/// When enabled, a cgroup OOM kills its tasks and descendant tasks together,
/// except OOM-protected tasks.
///
/// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type MemoryOomGroup = Scalar<bool, interface::MemoryOomGroup>;

/// Hierarchical memory usage reported by `memory.current`.
///
/// Usage is in bytes and can temporarily exceed configured limits.
/// The public snapshot aliases support parsing, typed values, filenames, and
/// value traits without exposing their shared storage implementation.
///
/// ```
/// use std::{fmt::Debug, hash::Hash, str::FromStr};
///
/// use sakai::{
///   Bytes, Count, MaxOr, Ratio, Time,
///   error::{ParseError, ParseValueError},
///   v2::{
///     cpu::{CpuIdle, CpuMaxBurst, CpuUclampMax},
///     memory::{MemoryCurrent, MemoryMax},
///     pids::PidsCurrent,
///   },
/// };
///
/// struct Consumer;
/// impl Consumer {
///   fn parse<T>(input: &str) -> T
///   where
///     T: Copy + PartialEq + Debug + FromStr<Err = ParseError<ParseValueError>>, {
///     input.parse().unwrap()
///   }
///
///   fn hashable<T: Eq + Hash>(_: T) {}
/// }
///
/// let current = Consumer::parse::<MemoryCurrent>("42");
/// Consumer::hashable(current);
/// let _: Bytes = current.value();
/// let _: MaxOr<Bytes> = Consumer::parse::<MemoryMax>("max").value();
/// let _: Count = Consumer::parse::<PidsCurrent>("1").value();
/// let _: bool = Consumer::parse::<CpuIdle>("0").value();
/// let _: Time = Consumer::parse::<CpuMaxBurst>("10").value();
/// let _: MaxOr<Ratio> = Consumer::parse::<CpuUclampMax>("max").value();
/// assert_eq!(MemoryCurrent::FILE_NAME, "memory.current");
/// ```
///
/// Equal units do not make different interfaces interchangeable:
///
/// ```compile_fail,E0308
/// use sakai::v2::memory::{MemoryCurrent, MemoryLow};
/// let current: MemoryCurrent = "1".parse().unwrap();
/// let low: MemoryLow = current;
/// ```
///
/// See [Linux `memory.current` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type MemoryCurrent = Scalar<Bytes, interface::MemoryCurrent>;

/// Peak hierarchical memory usage reported by `memory.peak`.
///
/// Since Linux 6.12, writes reset the peak for an individual file descriptor. This
/// reader opens a fresh read-only descriptor for each call, so it never resets
/// the value and observes the peak since cgroup creation. Older kernels may
/// lack this file; the reader then returns [`crate::Error::FileMissing`].
/// Peak values are bytes. The reset behavior is documented in the
/// [Linux 6.12 guide](https://github.com/torvalds/linux/blob/v6.12/Documentation/admin-guide/cgroup-v2.rst).
///
/// See [Linux `memory.peak` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
///
/// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/Documentation/admin-guide/cgroup-v2.rst).
/// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
pub type MemoryPeak = Scalar<Bytes, interface::MemoryPeak>;

/// The hard memory limit configured by `memory.max`.
///
/// `max` means no limit is configured here; ancestor limits can still apply.
///
/// See [Linux `memory.max` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type MemoryMax = Scalar<MaxOr<Bytes>, interface::MemoryMax>;

/// The memory throttling limit configured by `memory.high`.
///
/// Crossing this boundary triggers reclaim and throttling, not an OOM kill.
/// `max` means no limit is configured here; ancestor limits can still apply.
///
/// See [Linux `memory.high` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type MemoryHigh = Scalar<MaxOr<Bytes>, interface::MemoryHigh>;

/// The best-effort memory protection configured by `memory.low`.
///
/// Reclaim avoids memory below the effective low boundary while unprotected
/// memory is available. Ancestor settings can reduce the effective protection.
///
/// See [Linux `memory.low` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type MemoryLow = Scalar<Bytes, interface::MemoryLow>;

/// The hard memory protection configured by `memory.min`.
///
/// Memory below the effective min boundary cannot be reclaimed. If no
/// unprotected reclaimable memory remains, an OOM kill can follow. Ancestor
/// settings can reduce the effective protection.
///
/// See [Linux `memory.min` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type MemoryMin = Scalar<Bytes, interface::MemoryMin>;
