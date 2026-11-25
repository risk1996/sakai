//! Shared storage for typed, single-value cgroup snapshots.

use std::{any::type_name, fmt, marker::PhantomData, str::FromStr};

use crate::{
  error::{EncodeError, ParseError, ParseValueError},
  limit::MaxOr,
  parse::{EncodeCgroup, ParseBoolean, ParseBytes, ParseCgroup, ParseCount, ParseMicroseconds, Parser, Percent},
  unit::{Bytes, Count, Ratio, Time},
  v2::{CgroupFile, ReadableFile, WritableFile},
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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Scalar<T, I> {
  /// The stored quantity or limit. Editing it only changes this value.
  ///
  /// See [Linux cgroup file formats](https://docs.kernel.org/admin-guide/cgroup-v2.html#format).
  pub value: T,
  interface: PhantomData<I>,
}

impl<T: Copy, I: Interface> Scalar<T, I> {
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

type ScalarEncoding<T> = <T as ScalarValue>::Encoding;

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
  type Encoding = Percent;
}
impl<T: ScalarValue> ScalarValue for MaxOr<T> {
  type Encoding = T::Encoding;
}

impl<T, I: Interface> FromStr for Scalar<T, I>
where
  T: Copy + ScalarValue + ParseCgroup<ScalarEncoding<T>, Error = ParseValueError>,
{
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self::new(Parser::single::<ScalarEncoding<T>, T>(contents, I::FIELD)?))
  }
}

#[expect(private_bounds, reason = "encoding markers are internal to supported scalar types")]
impl<T: Copy + ScalarValue + EncodeCgroup<ScalarEncoding<T>>, I: Interface> Scalar<T, I> {
  /// Encodes the value as a canonical cgroup token, with no trailing newline.
  /// See [Linux cgroup file formats](https://docs.kernel.org/admin-guide/cgroup-v2.html#format).
  ///
  /// # Errors
  ///
  /// Returns an error if the value cannot be represented exactly in the
  /// interface's text format.
  pub fn encode_cgroup(&self) -> Result<String, EncodeError> {
    let mut output = String::new();
    EncodeCgroup::<ScalarEncoding<T>>::encode_field(&self.value, &mut output, I::FIELD)?;
    Ok(output)
  }
}

impl<T, I: Interface> crate::cgroup::common::interface::sealed::File for Scalar<T, I> {}
impl<T, I: Interface> CgroupFile for Scalar<T, I> {
  type Key = ();
  type Version = crate::v2::Version;

  const FILE_NAME: &'static str = I::FILE_NAME;
}

impl<T, I: Interface> ReadableFile for Scalar<T, I>
where
  Self: FromStr<Err = ParseError<ParseValueError>>,
{
  type Value = Self;

  fn decode(contents: &str) -> Result<Self, ParseError<ParseValueError>> { contents.parse() }
}

impl<T: Copy + ScalarValue + EncodeCgroup<ScalarEncoding<T>>, I: WritableScalar> WritableFile for Scalar<T, I> {
  type Command<'a> = T;

  fn encode(value: T) -> Result<String, EncodeError> { Self::new(value).encode_cgroup() }
}

trait WritableScalar: Interface {}

/// Interface markers used by the public snapshot aliases.
pub mod interface {
  use super::{Interface, WritableScalar};

  macro_rules! scalar_interface {
    ($(#[$doc:meta])* $name:ident, $file:literal, $field:literal $(, $writable:ident)?) => {
      $(#[$doc])*
      #[derive(Clone, Copy, PartialEq, Eq, Hash)]
      pub struct $name;

      impl Interface for $name {
        const FIELD: &'static str = $field;
        const FILE_NAME: &'static str = $file;
      }

      $(scalar_interface!(@write $name, $writable);)?
    };
    (@write $name:ident, writable) => { impl WritableScalar for $name {} };
  }

  scalar_interface!(
    /// Marker for the cgroup's own freeze request.
    CgroupFreeze,
    "cgroup.freeze",
    "freeze",
    writable
  );
  scalar_interface!(
    /// Marker for the configured descendant depth limit.
    CgroupMaxDepth,
    "cgroup.max.depth",
    "depth",
    writable
  );
  scalar_interface!(
    /// Marker for the configured descendant cgroup count limit.
    CgroupMaxDescendants,
    "cgroup.max.descendants",
    "descendants",
    writable
  );
  scalar_interface!(
    /// Marker for the local pressure accounting enable flag.
    CgroupPressure,
    "cgroup.pressure",
    "pressure",
    writable
  );
  scalar_interface!(PidsCurrent, "pids.current", "current");
  scalar_interface!(
    /// Marker for the historical task count peak.
    PidsPeak,
    "pids.peak",
    "peak"
  );
  scalar_interface!(PidsMax, "pids.max", "max", writable);
  scalar_interface!(MemoryOomGroup, "memory.oom.group", "oom.group", writable);
  scalar_interface!(MemoryCurrent, "memory.current", "current");
  scalar_interface!(MemoryPeak, "memory.peak", "peak");
  scalar_interface!(MemoryMax, "memory.max", "max", writable);
  scalar_interface!(MemoryHigh, "memory.high", "high", writable);
  scalar_interface!(MemoryLow, "memory.low", "low", writable);
  scalar_interface!(MemoryMin, "memory.min", "min", writable);
  scalar_interface!(SwapCurrent, "memory.swap.current", "current");
  scalar_interface!(SwapPeak, "memory.swap.peak", "peak");
  scalar_interface!(SwapMax, "memory.swap.max", "max", writable);
  scalar_interface!(SwapHigh, "memory.swap.high", "high", writable);
  scalar_interface!(ZswapCurrent, "memory.zswap.current", "current");
  scalar_interface!(ZswapMax, "memory.zswap.max", "max", writable);
  scalar_interface!(ZswapWriteback, "memory.zswap.writeback", "writeback", writable);
  scalar_interface!(CpuIdle, "cpu.idle", "idle", writable);
  scalar_interface!(CpuMaxBurst, "cpu.max.burst", "burst", writable);
  scalar_interface!(CpuUclampMin, "cpu.uclamp.min", "utilization", writable);
  scalar_interface!(CpuUclampMax, "cpu.uclamp.max", "utilization", writable);
}

#[cfg(test)]
mod tests;
