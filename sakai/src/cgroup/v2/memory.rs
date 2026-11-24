//! Memory controller interface files. Reads return fresh, separate snapshots.
//! Scalar memory amounts are bytes; limits and protections describe this cgroup's settings.

pub use events::*;
pub use numa_stat::*;
pub use stat::*;
pub use swap::*;
pub use zswap::*;

use crate::{
  limit::MaxOr,
  pressure::Pressure,
  scalar::{Scalar, interface},
  unit::Bytes,
};

mod events;
mod numa_stat;
mod stat;
mod swap;
mod zswap;

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
/// `max` requests protection for all memory charged to this cgroup; it does not
/// bypass ancestor boundaries or protection shared with siblings.
///
/// See [Linux `memory.low` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type MemoryLow = Scalar<MaxOr<Bytes>, interface::MemoryLow>;

/// The hard memory protection configured by `memory.min`.
///
/// Memory below the effective min boundary cannot be reclaimed. If no
/// unprotected reclaimable memory remains, an OOM kill can follow. Ancestor
/// settings can reduce the effective protection.
/// `max` requests protection for all memory charged to this cgroup; it does not
/// bypass ancestor boundaries or protection shared with siblings.
///
/// See [Linux `memory.min` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type MemoryMin = Scalar<MaxOr<Bytes>, interface::MemoryMin>;

crate::v2::interface::cgroup_file!(alias
  /// Memory pressure snapshots.
  MemoryPressure, Pressure::MEMORY_FILE_NAME, Read, Pressure
);
