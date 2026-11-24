//! Per-device I/O accounting, pressure, and configuration snapshots.
//!
//! Device numbers identify block devices, not filesystem paths. A device can
//! disappear between reads; no cross-file consistency is implied.

use std::{collections::BTreeMap, str::FromStr};

pub use cost::{IoCostControl, IoCostModel, IoCostModelName, IoCostQos, IoDeviceCostModel, IoDeviceCostQos};
pub use latency::IoLatency;
pub use max::{IoDeviceMax, IoMax, IoMaxCommand, IoMaxUpdate};
pub use priority::IoPriorityClass;
pub use stat::{DelayUsage, IoDeviceStat, IoStat};
pub use weight::{IoWeight, IoWeightCommand, Weight};

use crate::{
  Pressure,
  error::{ParseError, ParseValueError},
  parse::{KeyedFields, cgroup_value},
  v2::interface::cgroup_file,
};

mod cost;
mod latency;
mod max;
mod priority;
mod stat;
mod weight;

cgroup_file!(alias
  /// I/O pressure snapshots from `io.pressure`.
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  IoPressure, "io.pressure", Read, Pressure
);

/// A block-device identifier keyed by the kernel's decimal `major:minor` pair.
///
/// Device numbers identify a block device, not a mount or a filename.
/// See [makedev(3)](https://man7.org/linux/man-pages/man3/makedev.3.html).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct Device {
  /// Major device number.
  ///
  /// See [`Device`] and [makedev(3)](https://man7.org/linux/man-pages/man3/makedev.3.html).
  pub major: u32,
  /// Minor device number.
  ///
  /// See [`Device`] and [makedev(3)](https://man7.org/linux/man-pages/man3/makedev.3.html).
  pub minor: u32,
}

cgroup_value! { display Device }

impl Device {
  /// Constructs a device key without resolving it to a currently present device.
  ///
  /// See [`Device`] and [makedev(3)](https://man7.org/linux/man-pages/man3/makedev.3.html).
  #[must_use]
  pub const fn new(major: u32, minor: u32) -> Self { Self { major, minor } }

  fn parse(raw: &str, value: &str) -> Result<Self, ParseError<ParseValueError>> {
    let (major, minor) = value
      .split_once(DEVICE_SEPARATOR)
      .ok_or_else(|| ParseError::invalid(raw, "device", value, ParseValueError::OutOfRange))?;
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

const DEVICE_SEPARATOR: char = ':';

impl std::fmt::Display for Device {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(formatter, "{}{DEVICE_SEPARATOR}{}", self.major, self.minor)
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
