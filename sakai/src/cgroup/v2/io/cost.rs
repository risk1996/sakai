use std::{collections::BTreeMap, str::FromStr};

use nutype::nutype;
use uom::si::ratio::percent;

use super::{
  Device,
  max::{ParseBandwidth, ParseIops},
};
use crate::{
  error::{ParseError, ParseValueError},
  parse::{KeyedFields, ParseBoolean, ParseCgroup, ParseMicroseconds, ParsePercent},
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
  devices: BTreeMap<Device, IoDeviceCostQos>,
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
  enabled: bool,
  control: IoCostControl,
  read_percentile: Ratio,
  read_latency: Time,
  write_percentile: Ratio,
  write_latency: Time,
  min_scaling: Ratio,
  max_scaling: Ratio,
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

  fn from_fields(fields: KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    Ok(Self {
      enabled: fields.required::<ParseBoolean, _>("enable")?,
      control: fields.required::<IoCostControl, _>("ctrl")?,
      read_percentile: fields.required::<ParsePercent, _>("rpct")?,
      read_latency: fields.required::<ParseMicroseconds, _>("rlat")?,
      write_percentile: fields.required::<ParsePercent, _>("wpct")?,
      write_latency: fields.required::<ParseMicroseconds, _>("wlat")?,
      min_scaling: fields.required::<ParseScalingPercent, _>("min")?,
      max_scaling: fields.required::<ParseScalingPercent, _>("max")?,
    })
  }
}

impl FromStr for IoCostQos {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self { devices: Device::parse_lines(contents, IoDeviceCostQos::from_fields)? })
  }
}

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
  devices: BTreeMap<Device, IoDeviceCostModel>,
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
  control: IoCostControl,
  model: IoCostModelName,
  read_bandwidth: BytesPerSecond,
  read_sequential_iops: OperationsPerSecond,
  read_random_iops: OperationsPerSecond,
  write_bandwidth: BytesPerSecond,
  write_sequential_iops: OperationsPerSecond,
  write_random_iops: OperationsPerSecond,
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

  fn from_fields(fields: KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    Ok(Self {
      control: fields.required::<IoCostControl, _>("ctrl")?,
      model: fields.required::<IoCostModelName, _>("model")?,
      read_bandwidth: fields.required::<ParseBandwidth, _>("rbps")?,
      read_sequential_iops: fields.required::<ParseIops, _>("rseqiops")?,
      read_random_iops: fields.required::<ParseIops, _>("rrandiops")?,
      write_bandwidth: fields.required::<ParseBandwidth, _>("wbps")?,
      write_sequential_iops: fields.required::<ParseIops, _>("wseqiops")?,
      write_random_iops: fields.required::<ParseIops, _>("wrandiops")?,
    })
  }
}

impl FromStr for IoCostModel {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self { devices: Device::parse_lines(contents, IoDeviceCostModel::from_fields)? })
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
