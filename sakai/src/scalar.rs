//! Shared storage for typed, single-value cgroup snapshots.

use std::{any::type_name, fmt, marker::PhantomData, str::FromStr};

use crate::{
  error::{EncodeError, ParseError, ParseValueError},
  limit::MaxOr,
  parse::{EncodeCgroup, ParseBoolean, ParseBytes, ParseCgroup, ParseCount, ParseMicroseconds, ParsePercent, Parser},
  unit::{Bytes, Count, Ratio, Time},
  v2::{AccessMode, CgroupFile, FileCommand, FileValue},
};

/// The filename and diagnostic label of a scalar interface.
pub trait Interface: Copy {
  const FILE_NAME: &'static str;
  const FIELD: &'static str;
  type Mode: AccessMode;
}

/// A typed scalar snapshot, distinguished by its interface marker.
///
/// Values are parsed without I/O. Controller readers obtain fresh snapshots.
/// Different interfaces retain different types even when their units match.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Scalar<T, I> {
  /// The stored quantity or limit. Editing it only changes this value.
  pub value: T,
  interface: PhantomData<I>,
}

impl<T: Copy, I: Interface> Scalar<T, I> {
  /// The cgroup v2 interface filename.
  pub const FILE_NAME: &'static str = I::FILE_NAME;

  /// Returns the snapshot in its declared units.
  ///
  /// This inspects stored data; it does not read the kernel.
  ///
  /// See [Linux cgroup file formats](https://docs.kernel.org/admin-guide/cgroup-v2.html#format).
  #[must_use]
  pub const fn value(&self) -> T { self.value }

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
  type Encoding = ParsePercent;
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

impl<T, I: Interface> crate::v2::interface::sealed::File for Scalar<T, I> {}
impl<T, I: Interface> CgroupFile for Scalar<T, I> {
  type Key = ();
  type Mode = I::Mode;

  const FILE_NAME: &'static str = I::FILE_NAME;
}

impl<T, I: Interface> FileValue for Scalar<T, I>
where
  Self: FromStr<Err = ParseError<ParseValueError>>,
{
  type Value = Self;

  fn decode(contents: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> { Ok(contents.parse()?) }
}

impl<T: Copy + ScalarValue + EncodeCgroup<ScalarEncoding<T>>, I: Interface<Mode: crate::v2::WriteMode>> FileCommand
  for Scalar<T, I>
{
  type Command<'a> = T;

  fn encode(value: T) -> Result<String, EncodeError> { Self::new(value).encode_cgroup() }
}

/// Interface markers used by the public snapshot aliases.
pub mod interface {
  use super::Interface;
  use crate::v2::mode;

  macro_rules! scalar_interface {
    ($(#[$doc:meta])* $name:ident, $file:literal, $field:literal, $mode:ident) => {
      $(#[$doc])*
      #[derive(Clone, Copy, PartialEq, Eq, Hash)]
      pub struct $name;

      impl Interface for $name {
        const FIELD: &'static str = $field;
        const FILE_NAME: &'static str = $file;
        type Mode = mode::$mode;
      }
    };
  }

  scalar_interface!(
    /// Marker for the cgroup's own freeze request.
    CgroupFreeze,
    "cgroup.freeze",
    "freeze",
    ReadWrite
  );
  scalar_interface!(
    /// Marker for the configured descendant depth limit.
    CgroupMaxDepth,
    "cgroup.max.depth",
    "depth",
    ReadWrite
  );
  scalar_interface!(
    /// Marker for the configured descendant cgroup count limit.
    CgroupMaxDescendants,
    "cgroup.max.descendants",
    "descendants",
    ReadWrite
  );
  scalar_interface!(
    /// Marker for the local pressure accounting enable flag.
    CgroupPressure,
    "cgroup.pressure",
    "pressure",
    ReadWrite
  );
  scalar_interface!(PidsCurrent, "pids.current", "current", Read);
  scalar_interface!(
    /// Marker for the historical task count peak.
    PidsPeak,
    "pids.peak",
    "peak",
    Read
  );
  scalar_interface!(PidsMax, "pids.max", "max", ReadWrite);
  scalar_interface!(MemoryOomGroup, "memory.oom.group", "oom.group", ReadWrite);
  scalar_interface!(MemoryCurrent, "memory.current", "current", Read);
  scalar_interface!(MemoryPeak, "memory.peak", "peak", Read);
  scalar_interface!(MemoryMax, "memory.max", "max", ReadWrite);
  scalar_interface!(MemoryHigh, "memory.high", "high", ReadWrite);
  scalar_interface!(MemoryLow, "memory.low", "low", ReadWrite);
  scalar_interface!(MemoryMin, "memory.min", "min", ReadWrite);
  scalar_interface!(SwapCurrent, "memory.swap.current", "current", Read);
  scalar_interface!(SwapPeak, "memory.swap.peak", "peak", Read);
  scalar_interface!(SwapMax, "memory.swap.max", "max", ReadWrite);
  scalar_interface!(SwapHigh, "memory.swap.high", "high", ReadWrite);
  scalar_interface!(ZswapCurrent, "memory.zswap.current", "current", Read);
  scalar_interface!(ZswapMax, "memory.zswap.max", "max", ReadWrite);
  scalar_interface!(ZswapWriteback, "memory.zswap.writeback", "writeback", ReadWrite);
  scalar_interface!(CpuIdle, "cpu.idle", "idle", ReadWrite);
  scalar_interface!(CpuMaxBurst, "cpu.max.burst", "burst", ReadWrite);
  scalar_interface!(CpuUclampMin, "cpu.uclamp.min", "utilization", ReadWrite);
  scalar_interface!(CpuUclampMax, "cpu.uclamp.max", "utilization", ReadWrite);
}

#[cfg(test)]
mod tests;
