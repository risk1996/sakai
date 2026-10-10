use std::{collections::BTreeMap, str::FromStr};

use uom::si::{frequency::hertz, information_rate::byte_per_second};

use super::Device;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{KeyedFields, ParseCgroup},
  unit::{BytesPerSecond, OperationsPerSecond},
};

/// A configuration snapshot of per-device `io.max` limits.
///
/// Bandwidth is measured in bytes per second; IOPS in operations per second.
/// The file can be empty when no device limits have been configured. `max`
/// means unlimited at this cgroup; ancestors may still impose limits. Limits
/// can change between reads, and the kernel permits temporary bursts. Unknown
/// nested keys are ignored. All four limits are present in kernel read output,
/// even though writes accept partial updates.
///
/// ```
/// use sakai::{
///   MaxOr,
///   v2::io::{Device, IoMax},
/// };
/// use uom::si::{frequency::hertz, information_rate::byte_per_second};
///
/// let limits: IoMax = "8:0 rbps=1 wbps=max riops=1 wiops=max".parse()?;
/// let device = limits.devices().get(&Device::new(8, 0)).expect("reported device");
/// let MaxOr::Value(bandwidth) = device.read_bandwidth() else { panic!("finite bandwidth") };
/// let MaxOr::Value(iops) = device.read_iops() else { panic!("finite IOPS") };
/// assert_eq!(bandwidth.get::<byte_per_second>(), 1);
/// assert_eq!(iops.get::<hertz>(), 1);
/// # Ok::<(), sakai::error::ParseError<sakai::error::ParseValueError>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IoMax {
  devices: BTreeMap<Device, IoDeviceMax>,
}

impl IoMax {
  /// The cgroup v2 `io.max` interface filename.
  pub const FILE_NAME: &'static str = "io.max";

  /// Returns each reported device's configured limits.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<Device, IoDeviceMax> { &self.devices }
}

/// Read and write bandwidth and IOPS limits for a block device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoDeviceMax {
  read_bandwidth: MaxOr<BytesPerSecond>,
  write_bandwidth: MaxOr<BytesPerSecond>,
  read_iops: MaxOr<OperationsPerSecond>,
  write_iops: MaxOr<OperationsPerSecond>,
}

impl IoDeviceMax {
  /// Read bandwidth limit in bytes per second (`rbps`).
  #[must_use]
  pub const fn read_bandwidth(self) -> MaxOr<BytesPerSecond> { self.read_bandwidth }

  /// Write bandwidth limit in bytes per second (`wbps`).
  #[must_use]
  pub const fn write_bandwidth(self) -> MaxOr<BytesPerSecond> { self.write_bandwidth }

  /// Read operation limit in operations per second (`riops`).
  #[must_use]
  pub const fn read_iops(self) -> MaxOr<OperationsPerSecond> { self.read_iops }

  /// Write operation limit in operations per second (`wiops`).
  #[must_use]
  pub const fn write_iops(self) -> MaxOr<OperationsPerSecond> { self.write_iops }

  fn from_fields(fields: KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    Ok(Self {
      read_bandwidth: fields.required::<ParseBandwidth, _>("rbps")?,
      write_bandwidth: fields.required::<ParseBandwidth, _>("wbps")?,
      read_iops: fields.required::<ParseIops, _>("riops")?,
      write_iops: fields.required::<ParseIops, _>("wiops")?,
    })
  }
}

impl FromStr for IoMax {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self { devices: Device::parse_lines(contents, IoDeviceMax::from_fields)? })
  }
}

struct ParseBandwidth;

impl ParseCgroup<ParseBandwidth> for BytesPerSecond {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self::new::<byte_per_second>(value.parse()?)) }
}

struct ParseIops;

impl ParseCgroup<ParseIops> for OperationsPerSecond {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self::new::<hertz>(value.parse()?)) }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Invalid, Missing},
  };

  #[test]
  fn parses_device_limits_and_preserves_small_rates() {
    Cases::<IoMax>::check([
      (
        include_str!("../fixtures/p3/io.max"),
        Ok(IoMax {
          devices: BTreeMap::from([
            (Device::new(8, 16), IoDeviceMax {
              read_bandwidth: MaxOr::Value(BytesPerSecond::new::<byte_per_second>(2_097_152)),
              write_bandwidth: MaxOr::Max,
              read_iops: MaxOr::Max,
              write_iops: MaxOr::Value(OperationsPerSecond::new::<hertz>(120)),
            }),
            (Device::new(8, 0), IoDeviceMax {
              read_bandwidth: MaxOr::Value(BytesPerSecond::new::<byte_per_second>(1)),
              write_bandwidth: MaxOr::Value(BytesPerSecond::new::<byte_per_second>(u64::MAX)),
              read_iops: MaxOr::Value(OperationsPerSecond::new::<hertz>(1)),
              write_iops: MaxOr::Value(OperationsPerSecond::new::<hertz>(u64::MAX)),
            }),
          ]),
        }),
      ),
      (
        include_str!("../fixtures/p3_older/io.max"),
        Ok(IoMax {
          devices: BTreeMap::from([(Device::new(8, 0), IoDeviceMax {
            read_bandwidth: MaxOr::Max,
            write_bandwidth: MaxOr::Max,
            read_iops: MaxOr::Max,
            write_iops: MaxOr::Max,
          })]),
        }),
      ),
      ("", Ok(IoMax::default())),
      (" \n", Ok(IoMax::default())),
      (
        "8:0 rbps=0 wbps=0 riops=0 wiops=0",
        Ok(IoMax {
          devices: BTreeMap::from([(Device::new(8, 0), IoDeviceMax {
            read_bandwidth: MaxOr::Value(BytesPerSecond::new::<byte_per_second>(0)),
            write_bandwidth: MaxOr::Value(BytesPerSecond::new::<byte_per_second>(0)),
            read_iops: MaxOr::Value(OperationsPerSecond::new::<hertz>(0)),
            write_iops: MaxOr::Value(OperationsPerSecond::new::<hertz>(0)),
          })]),
        }),
      ),
      ("8:0", Err(Missing("value"))),
      ("8:0 wbps=max riops=max wiops=max", Err(Missing("rbps"))),
      ("8:0 rbps=max riops=max wiops=max", Err(Missing("wbps"))),
      ("8:0 rbps=max wbps=max wiops=max", Err(Missing("riops"))),
      ("8:0 rbps=max wbps=max riops=max", Err(Missing("wiops"))),
      ("8:0 rbps=-1 wbps=max riops=max wiops=max", Err(Invalid("rbps", "-1"))),
      ("8:0 rbps=max wbps=18446744073709551616 riops=max wiops=max", Err(Invalid("wbps", "18446744073709551616"))),
      ("8:0 rbps=max wbps=max riops=nope wiops=max", Err(Invalid("riops", "nope"))),
      ("8:0 rbps=max wbps=max riops=max wiops=18446744073709551616", Err(Invalid("wiops", "18446744073709551616"))),
      ("8:0 rbps=max wbps=max riops=max wiops=", Err(Invalid("wiops", ""))),
      ("8:0 rbps", Err(Invalid("value", "rbps"))),
      ("8:0:1 rbps=max", Err(Invalid("device", "8:0:1"))),
    ]);
  }
}
