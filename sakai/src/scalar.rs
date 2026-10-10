//! Shared storage for typed, single-value cgroup snapshots.

use std::{any::type_name, fmt, marker::PhantomData, str::FromStr};

use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{ParseBoolean, ParseBytes, ParseCgroup, ParseCount, ParseMicroseconds, ParsePercent, Parser},
  unit::{Bytes, Count, Ratio, Time},
};

/// The filename and diagnostic label of a scalar interface.
pub trait Interface: Copy {
  const FILE_NAME: &'static str;
  const FIELD: &'static str;
}

/// A typed scalar snapshot, distinguished by its interface marker.
///
/// Values are parsed without I/O. Controller readers obtain fresh snapshots.
/// Different interfaces retain different types even when their units match.
///
/// ```compile_fail
/// use sakai::v2::memory::{MemoryCurrent, MemoryLow};
/// let current: MemoryCurrent = "1".parse().unwrap();
/// let low: MemoryLow = current;
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Scalar<T, I> {
  value: T,
  interface: PhantomData<I>,
}

impl<T: Copy, I: Interface> Scalar<T, I> {
  /// The cgroup v2 interface filename.
  pub const FILE_NAME: &'static str = I::FILE_NAME;

  /// Returns the snapshot in its declared units.
  #[must_use]
  pub const fn value(self) -> T { self.value }

  pub(crate) const fn new(value: T) -> Self { Self { value, interface: PhantomData } }
}

impl<T: fmt::Debug, I> fmt::Debug for Scalar<T, I> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct(type_name::<I>().rsplit("::").next().unwrap_or("Scalar")).field("value", &self.value).finish()
  }
}

trait ScalarValue {
  type Encoding;
}

impl ScalarValue for bool {
  type Encoding = ParseBoolean;
}
impl ScalarValue for Bytes {
  type Encoding = ParseBytes;
}
impl ScalarValue for Count {
  type Encoding = ParseCount;
}
impl ScalarValue for Time {
  type Encoding = ParseMicroseconds;
}
impl ScalarValue for Ratio {
  type Encoding = ParsePercent;
}
impl<T: ScalarValue> ScalarValue for MaxOr<T> {
  type Encoding = T::Encoding;
}

impl<T, I: Interface> FromStr for Scalar<T, I>
where
  T: Copy + ScalarValue + ParseCgroup<T::Encoding, Error = ParseValueError>,
{
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self::new(Parser::single::<T::Encoding, T>(contents, I::FIELD)?))
  }
}

/// Interface markers used by the public snapshot aliases.
pub mod interface {
  use super::Interface;

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct PidsCurrent;
  impl Interface for PidsCurrent {
    const FIELD: &'static str = "current";
    const FILE_NAME: &'static str = "pids.current";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct PidsMax;
  impl Interface for PidsMax {
    const FIELD: &'static str = "max";
    const FILE_NAME: &'static str = "pids.max";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct MemoryOomGroup;
  impl Interface for MemoryOomGroup {
    const FIELD: &'static str = "oom.group";
    const FILE_NAME: &'static str = "memory.oom.group";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct MemoryCurrent;
  impl Interface for MemoryCurrent {
    const FIELD: &'static str = "current";
    const FILE_NAME: &'static str = "memory.current";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct MemoryPeak;
  impl Interface for MemoryPeak {
    const FIELD: &'static str = "peak";
    const FILE_NAME: &'static str = "memory.peak";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct MemoryMax;
  impl Interface for MemoryMax {
    const FIELD: &'static str = "max";
    const FILE_NAME: &'static str = "memory.max";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct MemoryHigh;
  impl Interface for MemoryHigh {
    const FIELD: &'static str = "high";
    const FILE_NAME: &'static str = "memory.high";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct MemoryLow;
  impl Interface for MemoryLow {
    const FIELD: &'static str = "low";
    const FILE_NAME: &'static str = "memory.low";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct MemoryMin;
  impl Interface for MemoryMin {
    const FIELD: &'static str = "min";
    const FILE_NAME: &'static str = "memory.min";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct SwapCurrent;
  impl Interface for SwapCurrent {
    const FIELD: &'static str = "current";
    const FILE_NAME: &'static str = "memory.swap.current";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct SwapPeak;
  impl Interface for SwapPeak {
    const FIELD: &'static str = "peak";
    const FILE_NAME: &'static str = "memory.swap.peak";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct SwapMax;
  impl Interface for SwapMax {
    const FIELD: &'static str = "max";
    const FILE_NAME: &'static str = "memory.swap.max";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct SwapHigh;
  impl Interface for SwapHigh {
    const FIELD: &'static str = "high";
    const FILE_NAME: &'static str = "memory.swap.high";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct ZswapCurrent;
  impl Interface for ZswapCurrent {
    const FIELD: &'static str = "current";
    const FILE_NAME: &'static str = "memory.zswap.current";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct ZswapMax;
  impl Interface for ZswapMax {
    const FIELD: &'static str = "max";
    const FILE_NAME: &'static str = "memory.zswap.max";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct ZswapWriteback;
  impl Interface for ZswapWriteback {
    const FIELD: &'static str = "writeback";
    const FILE_NAME: &'static str = "memory.zswap.writeback";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct CpuIdle;
  impl Interface for CpuIdle {
    const FIELD: &'static str = "idle";
    const FILE_NAME: &'static str = "cpu.idle";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct CpuMaxBurst;
  impl Interface for CpuMaxBurst {
    const FIELD: &'static str = "burst";
    const FILE_NAME: &'static str = "cpu.max.burst";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct CpuUclampMin;
  impl Interface for CpuUclampMin {
    const FIELD: &'static str = "utilization";
    const FILE_NAME: &'static str = "cpu.uclamp.min";
  }

  #[derive(Clone, Copy, PartialEq, Eq, Hash)]
  pub struct CpuUclampMax;
  impl Interface for CpuUclampMax {
    const FIELD: &'static str = "utilization";
    const FILE_NAME: &'static str = "cpu.uclamp.max";
  }
}

#[cfg(test)]
mod tests;
