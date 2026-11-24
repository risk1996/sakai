use std::{collections::BTreeMap, fmt::Write};

use uom::si::{frequency::hertz, information_rate::byte_per_second};

use super::Device;
use crate::{
  error::ParseValueError,
  limit::MaxOr,
  parse::{EncodeCgroup, ParseCgroup, cgroup_value},
  unit::{BytesPerSecond, OperationsPerSecond},
  v2::interface::cgroup_file,
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
/// let device = limits.devices.get(&Device::new(8, 0)).expect("reported device");
/// let MaxOr::Value(bandwidth) = device.read_bandwidth else { panic!("finite bandwidth") };
/// let MaxOr::Value(iops) = device.read_iops else { panic!("finite IOPS") };
/// assert_eq!(bandwidth.get::<byte_per_second>(), 1);
/// assert_eq!(iops.get::<hertz>(), 1);
/// # Ok::<(), sakai::error::ParseError<sakai::error::ParseValueError>>(())
/// ```
///
/// See [Linux `io.max` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct IoMax {
  /// Configured limits by block device.
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub devices: BTreeMap<Device, IoDeviceMax>,
}

cgroup_value! { device_rows IoMax, devices: IoDeviceMax, Device::parse_lines }
cgroup_file!(file IoMax, "io.max", ReadWrite);
cgroup_file!(read IoMax => IoMax);
cgroup_file!(write IoMax, 'a => IoMaxCommand<'a>, |command| match command {
  | IoMaxCommand::Set(device, limits) => {
    let mut command = device.to_string();
    limits.encode_fields(&mut command, " ", '=')?;
    Ok(command)
  },
  | IoMaxCommand::Update(device, update) => update.encode_command(device),
});

/// Read and write bandwidth and IOPS limits for a block device.
///
/// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct IoDeviceMax {
  /// Read bandwidth limit in bytes per second.
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub read_bandwidth: MaxOr<BytesPerSecond>,
  /// Write bandwidth limit in bytes per second.
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub write_bandwidth: MaxOr<BytesPerSecond>,
  /// Read I/O operations per second.
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub read_iops: MaxOr<OperationsPerSecond>,
  /// Write I/O operations per second.
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub write_iops: MaxOr<OperationsPerSecond>,
}

cgroup_value! { fields IoDeviceMax, {
  read_bandwidth: required ParseBandwidth => READ_BANDWIDTH,
  write_bandwidth: required ParseBandwidth => WRITE_BANDWIDTH,
  read_iops: required ParseIops => READ_IOPS,
  write_iops: required ParseIops => WRITE_IOPS,
} }

const READ_BANDWIDTH: &str = "rbps";
const WRITE_BANDWIDTH: &str = "wbps";
const READ_IOPS: &str = "riops";
const WRITE_IOPS: &str = "wiops";

/// A partial update to one device's `io.max` limits.
///
/// Omit a field to retain its kernel value. Use [`MaxOr::Max`] to clear a
/// limit. Supply at least one field when using [`IoMaxCommand::Update`].
/// See [Linux I/O limits](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct IoMaxUpdate {
  /// New read bandwidth in bytes per second, if supplied.
  pub read_bandwidth: Option<MaxOr<BytesPerSecond>>,
  /// New write bandwidth in bytes per second, if supplied.
  pub write_bandwidth: Option<MaxOr<BytesPerSecond>>,
  /// New read operations per second, if supplied.
  pub read_iops: Option<MaxOr<OperationsPerSecond>>,
  /// New write operations per second, if supplied.
  pub write_iops: Option<MaxOr<OperationsPerSecond>>,
}

impl IoMaxUpdate {
  fn encode_command(&self, device: Device) -> Result<String, crate::error::EncodeError> {
    let mut command = device.to_string();
    let start = command.len();
    for (key, field) in [(READ_BANDWIDTH, self.read_bandwidth), (WRITE_BANDWIDTH, self.write_bandwidth)] {
      if let Some(value) = field {
        crate::parse::encode_pair::<ParseBandwidth, _>(&mut command, " ", '=', key, &value)?;
      }
    }
    for (key, field) in [(READ_IOPS, self.read_iops), (WRITE_IOPS, self.write_iops)] {
      if let Some(value) = field {
        crate::parse::encode_pair::<ParseIops, _>(&mut command, " ", '=', key, &value)?;
      }
    }
    if command.len() == start {
      return Err(crate::error::EncodeError { field: "limits" });
    }
    Ok(command)
  }
}

pub(super) struct ParseBandwidth;

impl ParseCgroup<ParseBandwidth> for BytesPerSecond {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self::new::<byte_per_second>(value.parse()?)) }
}

pub(super) struct ParseIops;

impl ParseCgroup<ParseIops> for OperationsPerSecond {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self::new::<hertz>(value.parse()?)) }
}

impl EncodeCgroup<ParseBandwidth> for BytesPerSecond {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", self.get::<byte_per_second>()).ok() }
}

impl EncodeCgroup<ParseIops> for OperationsPerSecond {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", self.get::<hertz>()).ok() }
}

/// A complete or partial limit change for one device in `io.max`.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum IoMaxCommand<'a> {
  /// Replaces all four limits for the device.
  Set(Device, &'a IoDeviceMax),
  /// Changes only the supplied limits for the device.
  Update(Device, &'a IoMaxUpdate),
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Invalid, Missing},
  };

  impl IoDeviceMax {
    fn expected(bandwidth: [Option<u64>; 2], iops: [Option<u64>; 2]) -> Self {
      let [read_bandwidth, write_bandwidth] = bandwidth
        .map(|value| value.map_or(MaxOr::Max, |value| MaxOr::Value(BytesPerSecond::new::<byte_per_second>(value))));
      let [read_iops, write_iops] =
        iops.map(|value| value.map_or(MaxOr::Max, |value| MaxOr::Value(OperationsPerSecond::new::<hertz>(value))));
      Self { read_bandwidth, write_bandwidth, read_iops, write_iops }
    }
  }

  #[test]
  fn encodes_only_the_requested_device_limits() {
    let device = Device::new(8, 16);
    let update = IoMaxUpdate {
      read_bandwidth: Some(MaxOr::Max),
      write_iops: Some(MaxOr::Value(OperationsPerSecond::new::<hertz>(50))),
      ..IoMaxUpdate::default()
    };
    assert_eq!(assert_ok!(update.encode_command(device)), "8:16 rbps=max wiops=50");
    assert_eq!(assert_err!(IoMaxUpdate::default().encode_command(device)).field(), "limits");
  }

  #[test]
  fn parses_device_limits_and_preserves_small_rates() {
    Cases::<IoMax>::check([
      (
        include_str!("../fixtures/p3/io.max"),
        Ok(IoMax {
          devices: BTreeMap::from([
            (Device::new(8, 16), IoDeviceMax::expected([Some(2_097_152), None], [None, Some(120)])),
            (Device::new(8, 0), IoDeviceMax::expected([Some(1), Some(u64::MAX)], [Some(1), Some(u64::MAX)])),
          ]),
        }),
      ),
      (
        include_str!("../fixtures/p3_older/io.max"),
        Ok(IoMax { devices: BTreeMap::from([(Device::new(8, 0), IoDeviceMax::expected([None; 2], [None; 2]))]) }),
      ),
      ("", Ok(IoMax::default())),
      (" \n", Ok(IoMax::default())),
      (
        "8:0 rbps=0 wbps=0 riops=0 wiops=0",
        Ok(IoMax { devices: BTreeMap::from([(Device::new(8, 0), IoDeviceMax::expected([Some(0); 2], [Some(0); 2]))]) }),
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
