use std::{collections::BTreeMap, fmt::Write};

use nutype::nutype;
use uom::si::{ratio::percent, time::nanosecond};

use super::Device;
use crate::{
  error::ParseValueError,
  limit::MaxOr,
  parse::{EncodeCgroup, ParseBytes, ParseCgroup, ParseCount, ParseMicroseconds, cgroup_value},
  unit::{Bytes, Count, Ratio, Time},
  v2::interface::cgroup_file,
};

/// A volatile snapshot of hierarchical `io.stat` counters, keyed by device.
///
/// Bytes and operation counts accumulate independently; reads are not atomic
/// across devices. An empty file means no devices were reported. Older kernels
/// can omit discard counters or emit rows containing only controller debug
/// statistics. Missing counters remain `None`. Latency debug fields describe
/// this device group, rather than hierarchical cumulative usage. Unknown keys
/// are ignored. Cost-controller counters use modeled I/O time and include
/// descendants, rather than representing observed request completion latency.
///
/// See [Linux `io.stat` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct IoStat {
  /// Byte and operation counters for each reported device.
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub devices: BTreeMap<Device, IoDeviceStat>,
}

cgroup_value! { device_rows IoStat, devices: IoDeviceStat, Device::parse_lines }
cgroup_file!(IoStat, "io.stat", Read);

/// Cumulative byte and operation counters for one block device.
///
/// Absent fields are not assumed to be zero.
///
/// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub struct IoDeviceStat {
  /// Cumulative bytes read (`rbytes`).
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub read_bytes: Option<Bytes>,
  /// Cumulative bytes written (`wbytes`).
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub write_bytes: Option<Bytes>,
  /// Cumulative bytes discarded (`dbytes`), when reported.
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub discard_bytes: Option<Bytes>,
  /// Cumulative read operations (`rios`).
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub read_operations: Option<Count>,
  /// Cumulative write operations (`wios`).
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub write_operations: Option<Count>,
  /// Cumulative discard operations (`dios`), when reported.
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub discard_operations: Option<Count>,
  /// Delay-control indicator (`use_delay`), when kernel debug statistics are enabled.
  ///
  /// See [`DelayUsage`] and [Linux I/O latency](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-latency).
  pub delay_usage: Option<DelayUsage>,
  /// Current artificial delay (`delay_nsec`), already in kernel nanoseconds.
  ///
  /// This is a changing delay amount, not cumulative stall time.
  /// See [Linux I/O latency](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-latency).
  pub delay: Option<Time>,
  /// Current queue-depth limit (`depth`), preserving `max` when unrestricted.
  ///
  /// Reported only with I/O latency debug statistics enabled.
  /// See [Linux I/O latency fields](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-latency-interface-files).
  pub depth: Option<MaxOr<Count>>,
  /// Exponential moving average latency (`avg_lat`), converted from microseconds.
  ///
  /// Reported for rotational devices with I/O latency debug statistics enabled.
  /// See [Linux I/O latency fields](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-latency-interface-files).
  pub average_latency: Option<Time>,
  /// Sampling window (`win`), converted from kernel milliseconds.
  ///
  /// Reported for rotational devices; idle periods can extend the current window.
  /// See [Linux I/O latency fields](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-latency-interface-files).
  pub latency_window: Option<Time>,
  /// Non-rotational I/Os exceeding their target in the current window (`missed`).
  ///
  /// This live window counter can reset between reads; it is not cumulative.
  /// See [Linux I/O latency fields](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-latency-interface-files).
  pub missed: Option<Count>,
  /// Non-rotational I/Os accounted in the current window (`total`).
  ///
  /// This live window counter can reset between reads; it is not cumulative.
  /// See [Linux I/O latency fields](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-latency-interface-files).
  pub total: Option<Count>,
  /// Cumulative hierarchical modeled I/O usage (`cost.usage`), converted from microseconds.
  ///
  /// Requires enabled I/O cost control. This is modeled cost, not elapsed device service time.
  /// See the [Linux cost statistics source](https://github.com/torvalds/linux/blob/v5.10/block/blk-iocost.c).
  pub cost_usage: Option<Time>,
  /// Cumulative hierarchical I/O cost wait time (`cost.wait`), converted from microseconds.
  ///
  /// Requires enabled I/O cost control and kernel debug statistics.
  /// See the [Linux cost statistics source](https://github.com/torvalds/linux/blob/v5.10/block/blk-iocost.c).
  pub cost_wait: Option<Time>,
  /// Cumulative hierarchical time in I/O cost debt (`cost.indebt`), converted from microseconds.
  ///
  /// Requires enabled I/O cost control and kernel debug statistics.
  /// See the [Linux cost statistics source](https://github.com/torvalds/linux/blob/v5.10/block/blk-iocost.c).
  pub cost_indebt: Option<Time>,
  /// Cumulative hierarchical I/O cost delay time (`cost.indelay`), converted from microseconds.
  ///
  /// Requires enabled I/O cost control and kernel debug statistics.
  /// See the [Linux cost statistics source](https://github.com/torvalds/linux/blob/v5.10/block/blk-iocost.c).
  pub cost_indelay: Option<Time>,
  /// Current device virtual-time rate (`cost.vrate`), stored as a dimensionless ratio.
  ///
  /// Emitted only at the root with enabled I/O cost control. For example,
  /// kernel `150.00` (150%) becomes 1.5; this is not a measured bandwidth.
  /// See the [Linux cost statistics source](https://github.com/torvalds/linux/blob/v5.10/block/blk-iocost.c).
  pub cost_vrate: Option<Ratio>,
}

cgroup_value! { fields IoDeviceStat, {
  read_bytes: optional ParseBytes => "rbytes",
  write_bytes: optional ParseBytes => "wbytes",
  discard_bytes: optional ParseBytes => "dbytes",
  read_operations: optional ParseCount => "rios",
  write_operations: optional ParseCount => "wios",
  discard_operations: optional ParseCount => "dios",
  delay_usage: optional DelayUsage => "use_delay",
  delay: optional ParseNanoseconds => "delay_nsec",
  depth: optional ParseCount => "depth",
  average_latency: optional ParseMicroseconds => "avg_lat",
  latency_window: optional ParseMilliseconds => "win",
  missed: optional ParseCount => "missed",
  total: optional ParseCount => "total",
  cost_usage: optional ParseMicroseconds => "cost.usage",
  cost_wait: optional ParseMicroseconds => "cost.wait",
  cost_indebt: optional ParseMicroseconds => "cost.indebt",
  cost_indelay: optional ParseMicroseconds => "cost.indelay",
  cost_vrate: optional ParseCostPercent => "cost.vrate",
} }

/// The kernel's signed `use_delay` debug indicator.
///
/// `-1` requests a fixed delay without automatic scaling; nonnegative values
/// count users of the automatically scaled delay mechanism. This is a control
/// indicator rather than an I/O operation count. Use `as_ref` or `into_inner`
/// to inspect the exact encoded value.
/// See the [Linux delay implementation](https://github.com/torvalds/linux/blob/v6.8/block/blk-cgroup.c).
#[nutype(validate(greater_or_equal = -1), derive(Debug, Clone, Copy, PartialEq, Eq, AsRef))]
#[non_exhaustive]
pub struct DelayUsage(i32);

impl ParseCgroup<Self> for DelayUsage {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    Self::try_new(value.parse()?).map_err(|_error| ParseValueError::OutOfRange)
  }
}

struct ParseNanoseconds;

struct ParseCostPercent;

#[nutype(validate(finite, greater_or_equal = 0.0), derive(Debug, Clone, Copy))]
struct CostPercent(f64);

impl ParseCgroup<ParseCostPercent> for Ratio {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    let value = CostPercent::try_new(value.parse()?).map_err(|_error| ParseValueError::OutOfRange)?;
    Ok(Self::new::<percent>(value.into_inner()))
  }
}

impl ParseCgroup<ParseNanoseconds> for Time {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self::new::<nanosecond>(value.parse()?)) }
}

const NANOSECONDS_PER_MILLISECOND: u64 = 1_000_000;

struct ParseMilliseconds;

impl ParseCgroup<ParseMilliseconds> for Time {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    let nanos = value.parse::<u64>()?.checked_mul(NANOSECONDS_PER_MILLISECOND).ok_or(ParseValueError::OutOfRange)?;
    Ok(Self::new::<nanosecond>(nanos))
  }
}

impl EncodeCgroup<DelayUsage> for DelayUsage {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", self.as_ref()).ok() }
}

impl EncodeCgroup<ParseNanoseconds> for Time {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", self.get::<nanosecond>()).ok() }
}

impl EncodeCgroup<ParseMilliseconds> for Time {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    crate::parse::encode_scaled_time(self, output, NANOSECONDS_PER_MILLISECOND)
  }
}

impl EncodeCgroup<ParseCostPercent> for Ratio {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    crate::parse::encode_percent(self, output, |value| CostPercent::try_new(value).is_ok())
  }
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;
  use uom::si::{
    information::byte,
    time::{microsecond, millisecond},
  };

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Invalid, Missing},
  };

  impl IoDeviceStat {
    fn expected(bytes: [u64; 2], operations: [u64; 2], discard: Option<(u64, u64)>) -> Self {
      let [read_bytes, write_bytes] = bytes.map(|value| Some(Bytes::new::<byte>(value)));
      let [read_operations, write_operations] = operations.map(|value| Some(Count { value, ..Default::default() }));
      Self {
        read_bytes,
        write_bytes,
        read_operations,
        write_operations,
        discard_bytes: discard.map(|(value, _)| Bytes::new::<byte>(value)),
        discard_operations: discard.map(|(_, value)| Count { value, ..Default::default() }),
        ..Self::default()
      }
    }
  }

  #[test]
  fn parses_stat_snapshots() {
    Cases::<IoStat>::check([
      (
        include_str!("../fixtures/p3/io.stat"),
        Ok(IoStat {
          devices: BTreeMap::from([
            (Device::new(8, 16), IoDeviceStat::expected([1_459_200, 314_773_504], [192, 353], Some((0, 0)))),
            (
              Device::new(8, 0),
              IoDeviceStat::expected([90_430_464, 299_008_000], [8950, 1252], Some((50_331_648, 3021))),
            ),
          ]),
        }),
      ),
      (
        include_str!("../fixtures/p3_older/io.stat"),
        Ok(IoStat {
          devices: BTreeMap::from([
            (Device::new(8, 0), IoDeviceStat::expected([0, u64::MAX], [0, u64::MAX], None)),
            (Device::new(8, 16), IoDeviceStat {
              delay_usage: Some(assert_ok!(DelayUsage::try_new(1))),
              delay: Some(Time::new::<nanosecond>(12_345)),
              depth: Some(MaxOr::Value(Count { value: 1, ..Default::default() })),
              average_latency: Some(Time::new::<microsecond>(100)),
              latency_window: Some(Time::new::<millisecond>(100)),
              ..IoDeviceStat::default()
            }),
          ]),
        }),
      ),
      ("", Ok(IoStat::default())),
      (" \n\t", Ok(IoStat::default())),
      ("8:0", Err(Missing("value"))),
      ("8:0 rbytes", Err(Invalid("value", "rbytes"))),
      ("8:0 rbytes=", Err(Invalid("rbytes", ""))),
      ("8:0 rbytes=-1", Err(Invalid("rbytes", "-1"))),
      ("8:0 wbytes=nope", Err(Invalid("wbytes", "nope"))),
      ("8:0 dbytes=18446744073709551616", Err(Invalid("dbytes", "18446744073709551616"))),
      ("8:0 rios=-1", Err(Invalid("rios", "-1"))),
      ("8:0 wios=18446744073709551616", Err(Invalid("wios", "18446744073709551616"))),
      ("8:0 dios=nope", Err(Invalid("dios", "nope"))),
      ("sda rbytes=1", Err(Invalid("device", "sda"))),
    ]);
  }

  #[test]
  fn preserves_latency_debug_windows_and_fixed_delays() {
    Cases::<IoStat>::check([
      (
        "8:0 use_delay=-1 delay_nsec=18446744073709551615 depth=max missed=2 total=7",
        Ok(IoStat {
          devices: BTreeMap::from([(Device::new(8, 0), IoDeviceStat {
            delay_usage: Some(assert_ok!(DelayUsage::try_new(-1))),
            delay: Some(Time::new::<nanosecond>(u64::MAX)),
            depth: Some(MaxOr::Max),
            missed: Some(Count { value: 2, ..Default::default() }),
            total: Some(Count { value: 7, ..Default::default() }),
            ..IoDeviceStat::default()
          })]),
        }),
      ),
      ("8:0 use_delay=-2", Err(Invalid("use_delay", "-2"))),
      ("8:0 avg_lat=18446744073709552", Err(Invalid("avg_lat", "18446744073709552"))),
      ("8:0 win=18446744073710", Err(Invalid("win", "18446744073710"))),
      ("8:0 depth=nope", Err(Invalid("depth", "nope"))),
      (
        "8:0 cost.usage=1 cost.wait=2 cost.indebt=3 cost.indelay=4 cost.vrate=150.00",
        Ok(IoStat {
          devices: BTreeMap::from([(Device::new(8, 0), IoDeviceStat {
            cost_usage: Some(Time::new::<microsecond>(1)),
            cost_wait: Some(Time::new::<microsecond>(2)),
            cost_indebt: Some(Time::new::<microsecond>(3)),
            cost_indelay: Some(Time::new::<microsecond>(4)),
            cost_vrate: Some(Ratio::new::<percent>(150.0)),
            ..IoDeviceStat::default()
          })]),
        }),
      ),
      ("8:0 cost.vrate=NaN", Err(Invalid("cost.vrate", "NaN"))),
      ("8:0 cost.usage=18446744073709552", Err(Invalid("cost.usage", "18446744073709552"))),
    ]);
  }
}
