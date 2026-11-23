use std::{collections::BTreeMap, fmt::Write};

use nutype::nutype;
use uom::si::ratio::percent;

use super::{
  Device,
  max::{ParseBandwidth, ParseIops},
};
use crate::{
  error::ParseValueError,
  parse::{EncodeCgroup, ParseBoolean, ParseCgroup, ParseMicroseconds, ParsePercent, cgroup_value},
  unit::{BytesPerSecond, OperationsPerSecond, Ratio, Time},
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
pub struct IoCostQos {
  /// QoS settings by block device.
  pub devices: BTreeMap<Device, IoDeviceCostQos>,
}

impl IoCostQos {
  /// The root-only I/O cost QoS interface filename.
  pub const FILE_NAME: &'static str = "io.cost.qos";

  /// Inspects stored per-device parameters without kernel I/O.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<Device, IoDeviceCostQos> { &self.devices }
}

/// I/O cost controller enablement, latency targets, and scaling bounds for a device.
///
/// See [Linux I/O cost interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, Copy, PartialEq)]
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

impl IoDeviceCostQos {
  /// Whether weight-based I/O cost control is enabled for the device.
  #[must_use]
  pub const fn enabled(&self) -> bool { self.enabled }

  /// Whether the kernel or user controls the reported QoS parameters.
  #[must_use]
  pub const fn control(&self) -> IoCostControl { self.control }

  /// Read completion latency percentile, from zero to one; zero disables its use.
  #[must_use]
  pub const fn read_percentile(&self) -> Ratio { self.read_percentile }

  /// Read latency threshold, converted from kernel microseconds.
  #[must_use]
  pub const fn read_latency(&self) -> Time { self.read_latency }

  /// Write completion latency percentile, from zero to one; zero disables its use.
  #[must_use]
  pub const fn write_percentile(&self) -> Ratio { self.write_percentile }

  /// Write latency threshold, converted from kernel microseconds.
  #[must_use]
  pub const fn write_latency(&self) -> Time { self.write_latency }

  /// Minimum I/O issue-rate scaling, from 0.01 to 100 (1% to 10,000%).
  #[must_use]
  pub const fn min_scaling(&self) -> Ratio { self.min_scaling }

  /// Maximum I/O issue-rate scaling, from 0.01 to 100 (1% to 10,000%).
  #[must_use]
  pub const fn max_scaling(&self) -> Ratio { self.max_scaling }
}

cgroup_value! { fields IoDeviceCostQos, {
  enabled: required ParseBoolean => "enable",
  control: required IoCostControl => "ctrl",
  read_percentile: required ParsePercent => "rpct",
  read_latency: required ParseMicroseconds => "rlat",
  write_percentile: required ParsePercent => "wpct",
  write_latency: required ParseMicroseconds => "wlat",
  min_scaling: required ParseScalingPercent => "min",
  max_scaling: required ParseScalingPercent => "max",
} }

cgroup_value! { device_rows IoCostQos, devices: IoDeviceCostQos, Device::parse_lines }

struct ParseScalingPercent;

#[nutype(validate(finite, greater_or_equal = 1.0, less_or_equal = 10_000.0), derive(Debug, Clone, Copy))]
struct ScalingPercent(f64);

impl ParseCgroup<ParseScalingPercent> for Ratio {
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
pub struct IoCostModel {
  /// Cost models by block device.
  pub devices: BTreeMap<Device, IoDeviceCostModel>,
}

impl IoCostModel {
  /// The root-only I/O cost model interface filename.
  pub const FILE_NAME: &'static str = "io.cost.model";

  /// Inspects stored per-device models without kernel I/O.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<Device, IoDeviceCostModel> { &self.devices }
}

/// Device cost-model rates, with sequential and random IOPS measured for 4 KiB I/O.
///
/// See [Linux I/O cost interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl IoDeviceCostModel {
  /// Whether the kernel or user controls the reported model parameters.
  #[must_use]
  pub const fn control(&self) -> IoCostControl { self.control }

  /// The device's cost model.
  #[must_use]
  pub const fn model(&self) -> IoCostModelName { self.model }

  /// Maximum sequential read throughput used by the model, in bytes per second.
  #[must_use]
  pub const fn read_bandwidth(&self) -> BytesPerSecond { self.read_bandwidth }

  /// Maximum sequential 4 KiB read operations per second used by the model.
  #[must_use]
  pub const fn read_sequential_iops(&self) -> OperationsPerSecond { self.read_sequential_iops }

  /// Maximum random 4 KiB read operations per second used by the model.
  #[must_use]
  pub const fn read_random_iops(&self) -> OperationsPerSecond { self.read_random_iops }

  /// Maximum sequential write throughput used by the model, in bytes per second.
  #[must_use]
  pub const fn write_bandwidth(&self) -> BytesPerSecond { self.write_bandwidth }

  /// Maximum sequential 4 KiB write operations per second used by the model.
  #[must_use]
  pub const fn write_sequential_iops(&self) -> OperationsPerSecond { self.write_sequential_iops }

  /// Maximum random 4 KiB write operations per second used by the model.
  #[must_use]
  pub const fn write_random_iops(&self) -> OperationsPerSecond { self.write_random_iops }
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

cgroup_value! { device_rows IoCostModel, devices: IoDeviceCostModel, Device::parse_lines }

#[cfg(target_os = "linux")]
impl super::Io<'_> {
  /// Applies one device's QoS settings in one write at the hierarchy root.
  /// Automatic control writes the enable flag and control mode. User control
  /// also writes the latency targets and scaling bounds.
  /// See [Linux I/O cost interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_cost_qos(&self, device: Device, value: &IoDeviceCostQos) -> Result<(), crate::Error> {
    let command = match value.control {
      | IoCostControl::Auto => Ok(format!("{device} enable={} ctrl=auto", u8::from(value.enabled))),
      | IoCostControl::User => {
        let mut command = device.to_string();
        value.encode_fields(&mut command, " ", '=').map(|()| command)
      },
    };
    self.cgroup.write(IoCostQos::FILE_NAME, command)
  }

  /// Applies one device's cost model in one write at the hierarchy root.
  /// Automatic control lets the kernel choose the coefficients. User control
  /// supplies the model and all coefficients.
  /// See [Linux I/O cost interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_cost_model(&self, device: Device, value: &IoDeviceCostModel) -> Result<(), crate::Error> {
    let command = match value.control {
      | IoCostControl::Auto => Ok(format!("{device} ctrl=auto")),
      | IoCostControl::User => {
        let mut command = device.to_string();
        value.encode_fields(&mut command, " ", '=').map(|()| command)
      },
    };
    self.cgroup.write(IoCostModel::FILE_NAME, command)
  }
}

impl EncodeCgroup<IoCostControl> for IoCostControl {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{self}").ok() }
}

impl EncodeCgroup<IoCostModelName> for IoCostModelName {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{self}").ok() }
}

impl EncodeCgroup<ParseScalingPercent> for Ratio {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    crate::parse::encode_percent(self, output, |value| ScalingPercent::try_new(value).is_ok())
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
