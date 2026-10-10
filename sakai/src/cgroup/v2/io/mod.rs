//! Per-device I/O accounting, pressure, and configuration snapshots.
//!
//! Device numbers identify block devices, not filesystem paths. A device can
//! disappear between reads; no cross-file consistency is implied.

use std::{collections::BTreeMap, str::FromStr};

pub use latency::IoLatency;
pub use max::{IoDeviceMax, IoMax};
pub use stat::{IoDeviceStat, IoStat};
pub use weight::{IoWeight, Weight};

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::{Error, Pressure};
use crate::{
  error::{ParseError, ParseValueError},
  parse::KeyedFields,
};

mod latency;
mod max;
mod stat;
mod weight;

/// A borrowed view of one cgroup's I/O controller.
///
/// Every method opens a fresh read-only descriptor. Missing interfaces,
/// including disabled controllers, kernel configuration omissions, and root
/// exemptions, return [`crate::Error::FileMissing`].
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Io<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Io<'_> {
  /// Hierarchical byte and operation counters for each reported block device.
  pub fn stat(&self) -> Result<IoStat, Error> { self.cgroup.parse(IoStat::FILE_NAME) }

  /// I/O PSI rolling averages and cumulative stall time; never writes a trigger.
  pub fn pressure(&self) -> Result<Pressure, Error> { self.cgroup.parse(Pressure::IO_FILE_NAME) }

  /// Default scheduling weight and explicit per-device overrides.
  pub fn weight(&self) -> Result<IoWeight, Error> { self.cgroup.parse(IoWeight::FILE_NAME) }

  /// Per-device byte-per-second and operation-per-second limits.
  pub fn max(&self) -> Result<IoMax, Error> { self.cgroup.parse(IoMax::FILE_NAME) }

  /// Per-device latency protection targets, where the kernel supports them.
  pub fn latency(&self) -> Result<IoLatency, Error> { self.cgroup.parse(IoLatency::FILE_NAME) }
}

/// A block-device identifier keyed by the kernel's decimal `major:minor` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Device {
  major: u32,
  minor: u32,
}

impl Device {
  /// Constructs a device key without resolving it to a currently present device.
  #[must_use]
  pub const fn new(major: u32, minor: u32) -> Self { Self { major, minor } }

  /// Returns the major device number.
  #[must_use]
  pub const fn major(self) -> u32 { self.major }

  /// Returns the minor device number.
  #[must_use]
  pub const fn minor(self) -> u32 { self.minor }

  fn parse(raw: &str, value: &str) -> Result<Self, ParseError<ParseValueError>> {
    let (major, minor) =
      value.split_once(':').ok_or_else(|| ParseError::invalid(raw, "device", value, ParseValueError::OutOfRange))?;
    let parse =
      |part: &str| part.parse::<u32>().map_err(|error| ParseError::invalid(raw, "device", value, error.into()));
    Ok(Self::new(parse(major)?, parse(minor)?))
  }

  /// Reads the nested-keyed device format shared by I/O interfaces.
  fn parse_lines<T>(
    raw: &str,
    parse: impl Fn(KeyedFields<'_>) -> Result<T, ParseError<ParseValueError>>,
  ) -> Result<BTreeMap<Self, T>, ParseError<ParseValueError>> {
    raw
      .lines()
      .filter(|line| !line.trim().is_empty())
      .map(|line| {
        let mut tokens = line.split_ascii_whitespace();
        let device = Self::parse(raw, tokens.next().ok_or_else(|| ParseError::missing(raw, "device"))?)?;
        match tokens.clone().next() {
          | None => return Err(ParseError::missing(raw, "value")),
          | Some(_) => {},
        }
        let fields = KeyedFields::new(
          raw,
          tokens.map(|token| {
            token.split_once('=').ok_or_else(|| ParseError::invalid(raw, "value", token, ParseValueError::OutOfRange))
          }),
        )?;
        Ok((device, parse(fields)?))
      })
      .collect()
  }
}

impl FromStr for Device {
  type Err = ParseError<ParseValueError>;

  fn from_str(value: &str) -> Result<Self, Self::Err> { Self::parse(value, value) }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::parse::tests::{Cases, Failure::Invalid};

  #[test]
  fn parses_device_numbers() {
    Cases::<Device>::check([
      ("8:0", Ok(Device::new(8, 0))),
      ("259:1234", Ok(Device::new(259, 1234))),
      ("0:0", Ok(Device::new(0, 0))),
      ("4294967295:4294967295", Ok(Device::new(u32::MAX, u32::MAX))),
      ("8", Err(Invalid("device", "8"))),
      ("8:0:1", Err(Invalid("device", "8:0:1"))),
      (":0", Err(Invalid("device", ":0"))),
      ("8:", Err(Invalid("device", "8:"))),
      ("-1:0", Err(Invalid("device", "-1:0"))),
      ("8:-1", Err(Invalid("device", "8:-1"))),
      ("4294967296:0", Err(Invalid("device", "4294967296:0"))),
      ("8:4294967296", Err(Invalid("device", "8:4294967296"))),
      ("sda", Err(Invalid("device", "sda"))),
    ]);
  }
}
