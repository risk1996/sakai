use std::{collections::BTreeMap, fmt::Write};

use nutype::nutype;
use uom::si::ratio::percent;

use super::{
  Device,
  max::{ParseBandwidth, ParseIops},
};
use crate::{
  error::ParseValueError,
  parse::{EncodeCgroup, ParseBoolean, ParseCgroup, ParseMicroseconds, Percent, cgroup_value},
  unit::{BytesPerSecond, OperationsPerSecond, Ratio, Time},
  v2::interface::cgroup_file,
};

/// Who controls one device's I/O cost parameters.
///
/// See [Linux I/O cost interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[non_exhaustive]
pub enum IoCostControl {
  /// The kernel adjusts parameters automatically.
  Auto,
  /// Parameters were selected explicitly by a user.
  User,
}

impl ParseCgroup<Self> for IoCostControl {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    value.parse().map_err(|_error| ParseValueError::OutOfRange)
  }
}

/// The model used to estimate a device's I/O cost.
///
/// See [Linux I/O cost interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[non_exhaustive]
pub enum IoCostModelName {
  /// Linear costs estimated from throughput and sequential/random 4 KiB IOPS.
  Linear,
}

impl ParseCgroup<Self> for IoCostModelName {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    value.parse().map_err(|_error| ParseValueError::OutOfRange)
  }
}

/// A root-only snapshot of device quality-of-service parameters in `io.cost.qos`.
///
/// Requires `CONFIG_BLK_CGROUP_IOCOST`. Devices appear after their first cost
/// configuration write, so an empty file is valid. Automatic parameters may
/// change between reads. Percentages are stored as dimensionless ratios;
/// latency thresholds are converted from microseconds to [`Time`].
/// Unknown nested keys are ignored.
///
/// See [Linux I/O cost interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct IoCostQos {
  /// QoS settings by block device.
  pub devices: BTreeMap<Device, IoDeviceCostQos>,
}

cgroup_value! { device_rows IoCostQos, devices: IoDeviceCostQos, Device::parse_lines }
cgroup_file!(file IoCostQos, "io.cost.qos");
cgroup_file!(read IoCostQos => IoCostQos);
cgroup_file!(write IoCostQos, 'a => (Device, &'a IoDeviceCostQos), encode_qos);

/// I/O cost controller enablement, latency targets, and scaling bounds for a device.
///
/// See [Linux I/O cost interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct IoDeviceCostQos {
  /// Whether the I/O cost controller is enabled for this device.
  pub enabled: bool,
  /// Whether the kernel or user controls the parameters.
  pub control: IoCostControl,
  /// Read latency percentile as a ratio from zero to one.
  pub read_percentile: Ratio,
  /// Read latency target in whole microseconds.
  pub read_latency: Time,
  /// Write latency percentile as a ratio from zero to one.
  pub write_percentile: Ratio,
  /// Write latency target in whole microseconds.
  pub write_latency: Time,
  /// Minimum issue-rate scaling as a ratio from 0.01 to 100.
  pub min_scaling: Ratio,
  /// Maximum issue-rate scaling as a ratio from 0.01 to 100.
  pub max_scaling: Ratio,
}

cgroup_value! { fields IoDeviceCostQos, {
  enabled: required ParseBoolean => "enable",
  control: required IoCostControl => "ctrl",
  read_percentile: required Percent => "rpct",
  read_latency: required ParseMicroseconds => "rlat",
  write_percentile: required Percent => "wpct",
  write_latency: required ParseMicroseconds => "wlat",
  min_scaling: required ScalingPercent => "min",
  max_scaling: required ScalingPercent => "max",
} }

#[nutype(validate(finite, greater_or_equal = 1.0, less_or_equal = 10_000.0), derive(Debug, Clone, Copy))]
struct ScalingPercent(f64);

impl ParseCgroup<ScalingPercent> for Ratio {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    let value = ScalingPercent::try_new(value.parse()?).map_err(|_error| ParseValueError::OutOfRange)?;
    Ok(Self::new::<percent>(value.into_inner()))
  }
}

/// A root-only snapshot of the per-device model parameters in `io.cost.model`.
///
/// Requires `CONFIG_BLK_CGROUP_IOCOST`. An empty file is valid. Automatic
/// parameters may change between reads. Rates are model inputs, not measured
/// throughput or enforced limits. Unknown nested keys are ignored.
///
/// See [Linux I/O cost interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct IoCostModel {
  /// Cost models by block device.
  pub devices: BTreeMap<Device, IoDeviceCostModel>,
}

cgroup_value! { device_rows IoCostModel, devices: IoDeviceCostModel, Device::parse_lines }
cgroup_file!(file IoCostModel, "io.cost.model");
cgroup_file!(read IoCostModel => IoCostModel);
cgroup_file!(write IoCostModel, 'a => (Device, &'a IoDeviceCostModel), encode_model);

/// Device cost-model rates, with sequential and random IOPS measured for 4 KiB I/O.
///
/// See [Linux I/O cost interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct IoDeviceCostModel {
  /// Whether the kernel or user controls the cost model.
  pub control: IoCostControl,
  /// Cost model used by the kernel.
  pub model: IoCostModelName,
  /// Maximum sequential read throughput in bytes per second.
  pub read_bandwidth: BytesPerSecond,
  /// Maximum sequential 4 KiB reads per second.
  pub read_sequential_iops: OperationsPerSecond,
  /// Maximum random 4 KiB reads per second.
  pub read_random_iops: OperationsPerSecond,
  /// Maximum sequential write throughput in bytes per second.
  pub write_bandwidth: BytesPerSecond,
  /// Maximum sequential 4 KiB writes per second.
  pub write_sequential_iops: OperationsPerSecond,
  /// Maximum random 4 KiB writes per second.
  pub write_random_iops: OperationsPerSecond,
}

cgroup_value! { fields IoDeviceCostModel, {
  control: required IoCostControl => "ctrl",
  model: required IoCostModelName => "model",
  read_bandwidth: required ParseBandwidth => "rbps",
  read_sequential_iops: required ParseIops => "rseqiops",
  read_random_iops: required ParseIops => "rrandiops",
  write_bandwidth: required ParseBandwidth => "wbps",
  write_sequential_iops: required ParseIops => "wseqiops",
  write_random_iops: required ParseIops => "wrandiops",
} }

impl EncodeCgroup<IoCostControl> for IoCostControl {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{self}").ok() }
}

impl EncodeCgroup<IoCostModelName> for IoCostModelName {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{self}").ok() }
}

impl EncodeCgroup<ScalingPercent> for Ratio {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    crate::parse::encode_percent(self, output, |value| ScalingPercent::try_new(value).is_ok())
  }
}

fn encode_qos((device, value): (Device, &IoDeviceCostQos)) -> Result<String, crate::error::EncodeError> {
  match value.control {
    | IoCostControl::Auto => Ok(format!("{device} enable={} ctrl=auto", u8::from(value.enabled))),
    | IoCostControl::User => {
      let mut command = device.to_string();
      value.encode_fields(&mut command, " ", '=').map(|()| command)
    },
  }
}

fn encode_model((device, value): (Device, &IoDeviceCostModel)) -> Result<String, crate::error::EncodeError> {
  match value.control {
    | IoCostControl::Auto => Ok(format!("{device} ctrl=auto")),
    | IoCostControl::User => {
      let mut command = device.to_string();
      value.encode_fields(&mut command, " ", '=').map(|()| command)
    },
  }
}

#[cfg(test)]
mod tests {
  use uom::si::{frequency::hertz, information_rate::byte_per_second, time::microsecond};

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Invalid, Missing},
  };

  #[test]
  fn parses_cost_qos_scaling_above_one_hundred_percent_and_latency_units() {
    Cases::<IoCostQos>::check([
      ("", Ok(IoCostQos::default())),
      (
        "8:16 enable=1 ctrl=auto rpct=95.00 rlat=75000 wpct=95.00 wlat=150000 min=50.00 max=150.00 future=nope",
        Ok(IoCostQos {
          devices: BTreeMap::from([(Device::new(8, 16), IoDeviceCostQos {
            enabled: true,
            control: IoCostControl::Auto,
            read_percentile: Ratio::new::<percent>(95.0),
            read_latency: Time::new::<microsecond>(75_000),
            write_percentile: Ratio::new::<percent>(95.0),
            write_latency: Time::new::<microsecond>(150_000),
            min_scaling: Ratio::new::<percent>(50.0),
            max_scaling: Ratio::new::<percent>(150.0),
          })]),
        }),
      ),
      (
        "8:0 enable=0 ctrl=user rpct=0 rlat=0 wpct=100 wlat=0 min=1 max=10000",
        Ok(IoCostQos {
          devices: BTreeMap::from([(Device::new(8, 0), IoDeviceCostQos {
            enabled: false,
            control: IoCostControl::User,
            read_percentile: Ratio::new::<percent>(0.0),
            read_latency: Time::new::<microsecond>(0),
            write_percentile: Ratio::new::<percent>(100.0),
            write_latency: Time::new::<microsecond>(0),
            min_scaling: Ratio::new::<percent>(1.0),
            max_scaling: Ratio::new::<percent>(10_000.0),
          })]),
        }),
      ),
      ("8:0 enable=2", Err(Invalid("enable", "2"))),
      ("8:0 enable=1 ctrl=future", Err(Invalid("ctrl", "future"))),
      ("8:0 enable=1 ctrl=auto rpct=NaN", Err(Invalid("rpct", "NaN"))),
      ("8:0 enable=1 ctrl=auto rpct=100.01", Err(Invalid("rpct", "100.01"))),
      ("8:0 enable=1 ctrl=auto rpct=0 rlat=0 wpct=0 wlat=0 min=0", Err(Invalid("min", "0"))),
      ("8:0 enable=1 ctrl=auto rpct=0 rlat=0 wpct=0 wlat=0 min=1 max=10000.01", Err(Invalid("max", "10000.01"))),
      ("8:0 enable=1 ctrl=auto rpct=0 rlat=0 wpct=0 wlat=0 min=1", Err(Missing("max"))),
    ]);
  }

  #[test]
  fn parses_model_rates_without_rounding_small_values() {
    Cases::<IoCostModel>::check([
      ("", Ok(IoCostModel::default())),
      (
        "8:0 ctrl=user model=linear rbps=1 rseqiops=2 rrandiops=3 wbps=18446744073709551615 wseqiops=5 wrandiops=6",
        Ok(IoCostModel {
          devices: BTreeMap::from([(Device::new(8, 0), IoDeviceCostModel {
            control: IoCostControl::User,
            model: IoCostModelName::Linear,
            read_bandwidth: BytesPerSecond::new::<byte_per_second>(1),
            read_sequential_iops: OperationsPerSecond::new::<hertz>(2),
            read_random_iops: OperationsPerSecond::new::<hertz>(3),
            write_bandwidth: BytesPerSecond::new::<byte_per_second>(u64::MAX),
            write_sequential_iops: OperationsPerSecond::new::<hertz>(5),
            write_random_iops: OperationsPerSecond::new::<hertz>(6),
          })]),
        }),
      ),
      ("8:0 ctrl=auto model=future", Err(Invalid("model", "future"))),
      ("8:0 ctrl=auto model=linear rbps=-1", Err(Invalid("rbps", "-1"))),
      ("8:0 ctrl=auto model=linear rbps=1", Err(Missing("rseqiops"))),
    ]);
  }
}
