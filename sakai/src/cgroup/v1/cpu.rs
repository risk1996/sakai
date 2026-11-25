//! Fair scheduler shares and CPU bandwidth.
//!
//! See [Linux CFS bandwidth control](https://docs.kernel.org/scheduler/sched-bwc.html#management).

use super::codec::{CpuQuotaToken, Nanoseconds, Unsigned};
use crate::{
  Count, MaxOr, NonZeroTime, Time,
  parse::{ParseBandwidthCount, ParseNonZeroMicroseconds, cgroup_value},
};

v1_scalars! {
  /// Quota in microseconds per period. The kernel reports -1 for no local limit.
  /// Ancestor bandwidth limits still apply. Writes at the hierarchy root are rejected.
  CpuQuota, "cpu.cfs_quota_us", MaxOr<NonZeroTime>, CpuQuotaToken, ReadWrite;
  /// Length of a bandwidth enforcement period, encoded in microseconds.
  /// The kernel accepts periods from 1 millisecond through 1 second.
  /// Writes at the hierarchy root are rejected.
  CpuPeriod, "cpu.cfs_period_us", NonZeroTime, ParseNonZeroMicroseconds, ReadWrite;
  /// Extra runtime the group may accumulate, encoded in microseconds.
  /// Zero disables bursting. Writes at the hierarchy root are rejected.
  /// See the [Linux 5.15 scheduler implementation](https://github.com/torvalds/linux/blob/v5.15/kernel/sched/core.c).
  CpuBurst, "cpu.cfs_burst_us", Time, crate::parse::ParseMicroseconds, ReadWrite;
  /// Relative share of fair-scheduler CPU time among siblings. Defaults to 1024.
  /// The kernel clamps writes to its supported range.
  /// See [Linux scheduler cgroup controls](https://github.com/torvalds/linux/blob/v5.15/kernel/sched/core.c).
  CpuShares, "cpu.shares", u64, Unsigned, ReadWrite;
}

/// CPU bandwidth statistics for this cgroup. Counters can change between reads.
/// See [Linux CFS bandwidth statistics](https://docs.kernel.org/scheduler/sched-bwc.html#statistics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct CpuStat {
  /// Number of enforcement periods.
  pub periods: Count,
  /// Number of throttled enforcement periods.
  pub throttled_periods: Count,
  /// Aggregate throttling duration, reported in nanoseconds.
  pub throttled_time: Time,
  /// Number of periods in which runtime exceeded the quota, when reported.
  pub bursts: Option<Count>,
  /// Runtime above the quota, reported in nanoseconds when present.
  pub burst_time: Option<Time>,
}

cgroup_value! { keyed CpuStat, |_| "value", {
  periods: required ParseBandwidthCount => "nr_periods",
  throttled_periods: required ParseBandwidthCount => "nr_throttled",
  throttled_time: required Nanoseconds => "throttled_time",
  bursts: optional ParseBandwidthCount => "nr_bursts",
  burst_time: optional Nanoseconds => "burst_time",
} }
v1_file!(CpuStat, "cpu.stat", Read);

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use uom::si::time::{microsecond, nanosecond};

  use super::*;
  use crate::cgroup::WritableFile;

  #[test]
  fn keeps_v1_quota_sentinel_and_time_units() {
    for (raw, quota) in
      [("-1", MaxOr::Max), ("25000", MaxOr::Value(assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(25_000)))))]
    {
      assert_eq!(assert_ok!(raw.parse::<CpuQuota>()), CpuQuota { value: quota });
      assert_eq!(assert_ok!(CpuQuota::encode(quota)), raw);
    }
    for raw in ["max", "-2", "0", "20000 100000"] {
      assert_err!(raw.parse::<CpuQuota>());
    }
    let stat = assert_ok!("nr_periods 5\nnr_throttled 2\nthrottled_time 3001\n".parse::<CpuStat>());
    assert_eq!(stat, CpuStat {
      periods: Count { value: 5, ..Default::default() },
      throttled_periods: Count { value: 2, ..Default::default() },
      throttled_time: Time::new::<nanosecond>(3001),
      bursts: None,
      burst_time: None,
    });
  }
}
