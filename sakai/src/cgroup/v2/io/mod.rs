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

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::Error;
use crate::{
  Pressure,
  error::{ParseError, ParseValueError},
  parse::{KeyedFields, cgroup_value},
};

mod cost;
mod latency;
mod max;
mod priority;
mod stat;
mod weight;

/// A borrowed view of one cgroup's I/O controller.
///
/// Reads open fresh read-only descriptors. Missing interfaces,
/// including disabled controllers, kernel configuration omissions, and root
/// exemptions, return [`crate::Error::FileMissing`].
///
/// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Io<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Io<'_> {
  /// Hierarchical byte and operation counters for each reported block device.
  ///
  /// Reads a fresh [`IoStat`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn stat(&self) -> Result<IoStat, Error> { self.cgroup.read_value::<IoStat>() }

  /// I/O PSI rolling averages and cumulative stall time; never writes a trigger.
  ///
  /// Reads a fresh [`Pressure`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn pressure(&self) -> Result<Pressure, Error> { self.cgroup.read_value::<IoPressure>() }

  /// Default scheduling weight and explicit per-device overrides.
  ///
  /// Reads a fresh [`IoWeight`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn weight(&self) -> Result<IoWeight, Error> { self.cgroup.read_value::<IoWeight>() }

  /// Per-device byte-per-second and operation-per-second limits.
  ///
  /// Reads a fresh [`IoMax`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max(&self) -> Result<IoMax, Error> { self.cgroup.read_value::<IoMax>() }

  /// Per-device latency protection targets, where the kernel supports them.
  ///
  /// Reads a fresh [`IoLatency`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn latency(&self) -> Result<IoLatency, Error> { self.cgroup.read_value::<IoLatency>() }

  /// Reads the device I/O cost controller's QoS configuration at the root.
  ///
  /// Requires `CONFIG_BLK_CGROUP_IOCOST`; automatic values can change between reads.
  /// See [Linux I/O interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors);
  /// non-root cgroups lack this interface.
  pub fn cost_qos(&self) -> Result<IoCostQos, Error> { self.cgroup.read_value::<IoCostQos>() }

  /// Reads the device I/O cost models at the root.
  ///
  /// Requires `CONFIG_BLK_CGROUP_IOCOST`; model rates are inputs rather than measured usage.
  /// See [Linux I/O interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors);
  /// non-root cgroups lack this interface.
  pub fn cost_model(&self) -> Result<IoCostModel, Error> { self.cgroup.read_value::<IoCostModel>() }

  /// Reads the I/O request-priority policy, including at the root.
  ///
  /// Requires `CONFIG_BLK_CGROUP_IOPRIO`; see [`IoPriorityClass`].
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn priority_class(&self) -> Result<IoPriorityClass, Error> { self.cgroup.read_value::<IoPriorityClass>() }

  /// Sets the I/O request priority policy.
  /// See the [Linux I/O priority controller](https://github.com/torvalds/linux/blob/v6.5/block/blk-ioprio.c).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_priority_class(&self, value: IoPriorityClass) -> Result<(), Error> {
    self.cgroup.write_value::<IoPriorityClass>(&value)
  }
}

/// A block-device identifier keyed by the kernel's decimal `major:minor` pair.
///
/// Device numbers identify a block device, not a mount or a filename.
/// See [makedev(3)](https://man7.org/linux/man-pages/man3/makedev.3.html).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Device {
  major: u32,
  minor: u32,
}

cgroup_value! { display Device }

const DEVICE_SEPARATOR: char = ':';

impl Device {
  /// Constructs a device key without resolving it to a currently present device.
  ///
  /// See [`Device`] and [makedev(3)](https://man7.org/linux/man-pages/man3/makedev.3.html).
  #[must_use]
  pub const fn new(major: u32, minor: u32) -> Self { Self { major, minor } }

  /// Returns the major device number.
  ///
  /// See [`Device`] and [makedev(3)](https://man7.org/linux/man-pages/man3/makedev.3.html).
  #[must_use]
  pub const fn major(&self) -> u32 { self.major }

  /// Returns the minor device number.
  ///
  /// See [`Device`] and [makedev(3)](https://man7.org/linux/man-pages/man3/makedev.3.html).
  #[must_use]
  pub const fn minor(&self) -> u32 { self.minor }

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

crate::v2::interface::cgroup_file!(alias
  /// I/O pressure snapshots.
  IoPressure, Pressure::IO_FILE_NAME, Read, Pressure
);
