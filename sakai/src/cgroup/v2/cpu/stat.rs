use std::str::FromStr;

use crate::{
  error::{ParseError, ParseValueError},
  parse::{EncodeCgroup, KeyedFields, ParseCgroup, ParseMicroseconds, cgroup_value},
  unit::{Count, Time},
  v2::interface::cgroup_file,
};

/// CPU usage counters from a cgroup's `cpu.stat` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain fresh values. All durations are reported by the kernel in
/// microseconds and stored as [`Time`].
///
/// See [Linux `cpu.stat` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct CpuStat {
  /// CPU usage times.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub time: CpuTimeStat,
  /// CPU bandwidth counters when the CPU controller reports them.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub bandwidth: Option<CpuBandwidthStat>,
}

cgroup_file!(CpuStat, "cpu.stat", Read);

impl CpuStat {
  /// Encodes the stored snapshot as canonical cgroup text.
  ///
  /// # Errors
  ///
  /// Returns an error if a field cannot be represented exactly.
  pub fn encode_cgroup(&self) -> Result<String, crate::error::EncodeError> {
    let mut output = self.time.encode_cgroup()?;
    if let Some(bandwidth) = &self.bandwidth {
      output.push('\n');
      output.push_str(&bandwidth.encode_cgroup()?);
    }
    Ok(output)
  }
}

/// Runqueue throttling in `cpu.stat.local`, including ancestor bandwidth limits.
///
/// This is a live snapshot. An empty file means the controller reports no
/// local counter; an absent file is handled separately by the reader.
///
/// See [Linux `cpu.stat.local` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
///
/// Since Linux 6.6; see the [upstream Linux 6.6 source](https://github.com/torvalds/linux/blob/v6.6/kernel/cgroup/cgroup.c).
/// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct CpuStatLocal {
  /// Throttling of this cgroup's own runqueues, when reported.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub throttled: Option<Time>,
}

cgroup_value! { keyed CpuStatLocal, CpuStatField::label, {
  throttled: optional ParseMicroseconds => CpuStatField::ThrottledUsec,
} }
cgroup_file!(CpuStatLocal, "cpu.stat.local", Read);

/// CPU usage times from a cgroup's `cpu.stat` file.
///
/// See [Linux `cpu.stat` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct CpuTimeStat {
  /// Total CPU time consumed by the cgroup and its descendants.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub usage: Time,
  /// CPU time consumed in user mode.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub user: Time,
  /// CPU time consumed in kernel mode.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub system: Time,
  /// User time of positively niced tasks, already included in [`Self::user`].
  ///
  /// Since Linux 6.13; see the [Linux 6.13 source](https://github.com/torvalds/linux/blob/v6.13/kernel/cgroup/rstat.c).
  /// `None` means the counter was not reported, rather than measured zero.
  pub nice: Option<Time>,
  /// Sibling CPU time forced idle by core scheduling for this subtree.
  ///
  /// This is separate from CPU usage. Since Linux 6.0 and available with
  /// `CONFIG_SCHED_CORE`; see the [Linux 6.0 source](https://github.com/torvalds/linux/blob/v6.0/kernel/cgroup/rstat.c).
  /// `None` means the counter was not reported, rather than measured zero.
  pub force_idle: Option<Time>,
}

cgroup_value! { keyed CpuTimeStat, CpuStatField::label, {
  usage: required ParseMicroseconds => CpuStatField::UsageUsec,
  user: required ParseMicroseconds => CpuStatField::UserUsec,
  system: required ParseMicroseconds => CpuStatField::SystemUsec,
  nice: optional ParseMicroseconds => CpuStatField::NiceUsec,
  force_idle: optional ParseMicroseconds => CpuStatField::ForceIdleUsec,
} }

/// CPU bandwidth counters from a cgroup's `cpu.stat` file.
///
/// These describe fair-class bandwidth enforcement by this cgroup's own
/// limit. Use [`CpuStatLocal`] for runqueue throttling that can also include
/// ancestor limits.
/// The kernel's period, throttling, and burst event counters are 32-bit and
/// wrap; signed decimal output after the sign bit is set is exposed as the
/// corresponding unsigned count. Do not assume snapshots are monotonic forever.
///
/// See [Linux `cpu.stat` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files)
/// and the [counter implementation](https://github.com/torvalds/linux/blob/v6.13/kernel/sched/sched.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct CpuBandwidthStat {
  /// Number of elapsed enforcement periods.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub nr_periods: Count,
  /// Number of periods in which the cgroup was throttled.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub nr_throttled: Count,
  /// Total time for which the cgroup was throttled.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub throttled: Time,
  /// CPU burst counters when reported by the kernel.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Since Linux 5.16; see the [upstream Linux 5.16 source](https://github.com/torvalds/linux/blob/v5.16/kernel/sched/core.c).
  /// `None` means the kernel did not report burst counters.
  pub burst: Option<CpuBurstStat>,
}

cgroup_value! { keyed CpuBandwidthStat, CpuStatField::label, {
  burst: group CpuBurstStat => CpuStatField::bandwidth_burst_fields(),
  nr_periods: required ParseBandwidthCount => CpuStatField::NrPeriods,
  nr_throttled: required ParseBandwidthCount => CpuStatField::NrThrottled,
  throttled: required ParseMicroseconds => CpuStatField::ThrottledUsec,
} }

/// CPU burst counters from a cgroup's `cpu.stat` file.
///
/// See [Linux `cpu.stat` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
///
/// Since Linux 5.16; see the [upstream Linux 5.16 source](https://github.com/torvalds/linux/blob/v5.16/kernel/sched/core.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct CpuBurstStat {
  /// Number of periods in which a burst occurred.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub nr_bursts: Count,
  /// Cumulative CPU time consumed above quota during bursts.
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub burst: Time,
}

cgroup_value! { keyed CpuBurstStat, CpuStatField::label, {
  nr_bursts: required ParseBandwidthCount => CpuStatField::NrBursts,
  burst: required ParseMicroseconds => CpuStatField::BurstUsec,
} }

struct ParseBandwidthCount;

impl EncodeCgroup<ParseBandwidthCount> for Count {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    EncodeCgroup::<crate::parse::ParseCount>::encode_token(self, output)
  }
}

impl ParseCgroup<ParseBandwidthCount> for Count {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    let value = if value.starts_with('-') {
      u64::from(u32::from_ne_bytes(value.parse::<i32>()?.to_ne_bytes()))
    } else {
      value.parse()?
    };
    Ok(Self { value, ..Default::default() })
  }
}

impl FromStr for CpuStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = CpuStatField::fields(contents)?;

    Ok(Self {
      time: CpuTimeStat::from_fields(&fields)?,
      bandwidth: if fields.contains_any(CpuStatField::bandwidth_fields()) {
        Some(CpuBandwidthStat::from_fields(&fields)?)
      } else {
        None
      },
    })
  }
}

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::Display, strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
enum CpuStatField {
  UsageUsec,
  UserUsec,
  SystemUsec,
  NiceUsec,
  #[strum(serialize = "core_sched.force_idle_usec")]
  ForceIdleUsec,
  NrPeriods,
  NrThrottled,
  ThrottledUsec,
  NrBursts,
  BurstUsec,
}

impl CpuStatField {
  fn fields(raw: &str) -> Result<KeyedFields<'_>, ParseError<ParseValueError>> { KeyedFields::parse(raw, Self::label) }

  fn label(key: &str) -> &'static str { Self::from_str(key).map(Into::into).unwrap_or("value") }

  fn bandwidth_fields() -> [Self; 5] {
    [Self::NrPeriods, Self::NrThrottled, Self::ThrottledUsec, Self::NrBursts, Self::BurstUsec]
  }

  fn bandwidth_burst_fields() -> [Self; 2] { [Self::NrBursts, Self::BurstUsec] }
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;
  use indoc::indoc;
  use uom::si::time::microsecond;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

  impl CpuTimeStat {
    fn expected([usage, user, system]: [u64; 3]) -> Self {
      Self {
        usage: Time::new::<microsecond>(usage),
        user: Time::new::<microsecond>(user),
        system: Time::new::<microsecond>(system),
        nice: None,
        force_idle: None,
      }
    }
  }

  impl CpuBurstStat {
    fn expected([nr_bursts, burst]: [u64; 2]) -> Self {
      Self { nr_bursts: Count { value: nr_bursts, ..Default::default() }, burst: Time::new::<microsecond>(burst) }
    }
  }

  impl CpuBandwidthStat {
    fn expected([nr_periods, nr_throttled, throttled]: [u64; 3], burst: Option<CpuBurstStat>) -> Self {
      Self {
        nr_periods: Count { value: nr_periods, ..Default::default() },
        nr_throttled: Count { value: nr_throttled, ..Default::default() },
        throttled: Time::new::<microsecond>(throttled),
        burst,
      }
    }
  }

  #[test]
  fn parses_local_throttling_and_ignores_unknowns() {
    Cases::<CpuStatLocal>::check([
      ("", Ok(CpuStatLocal { throttled: None })),
      ("throttled_usec 123\n", Ok(CpuStatLocal { throttled: Some(Time::new::<microsecond>(123)) })),
      ("future 7\nthrottled_usec 0\n", Ok(CpuStatLocal { throttled: Some(Time::new::<microsecond>(0)) })),
      ("future nope\n", Ok(CpuStatLocal { throttled: None })),
      (
        "\n\t\nthrottled_usec nope\nthrottled_usec 123\n",
        Ok(CpuStatLocal { throttled: Some(Time::new::<microsecond>(123)) }),
      ),
      ("throttled_usec 123\nthrottled_usec nope", Err(Invalid("throttled_usec", "nope"))),
      ("throttled_usec nope", Err(Invalid("throttled_usec", "nope"))),
      ("throttled_usec 18446744073709552", Err(Invalid("throttled_usec", "18446744073709552"))),
      ("throttled_usec 1 extra", Err(Excess)),
      ("throttled_usec", Err(Missing("throttled_usec"))),
      ("future", Err(Missing("value"))),
    ]);
  }

  #[test]
  fn parses_cpu_stat() {
    Cases::<CpuStat>::check([
      (
        indoc! {"
          usage_usec 54321
          user_usec 32100
          system_usec 22221
        "},
        Ok(CpuStat { time: CpuTimeStat::expected([54_321, 32_100, 22_221]), bandwidth: None }),
      ),
      (
        indoc! {"
          usage_usec 54321
          user_usec 32100
          system_usec 22221
          nr_periods 100
          nr_throttled 7
          throttled_usec 1234
          nr_bursts 3
          burst_usec 456
        "},
        Ok(CpuStat {
          time: CpuTimeStat::expected([54_321, 32_100, 22_221]),
          bandwidth: Some(CpuBandwidthStat::expected([100, 7, 1_234], Some(CpuBurstStat::expected([3, 456])))),
        }),
      ),
      (
        indoc! {"
          usage_usec 1
          user_usec 2
          system_usec 3
          nr_periods 4
          nr_throttled 5
          throttled_usec 6
        "},
        Ok(CpuStat {
          time: CpuTimeStat::expected([1, 2, 3]),
          bandwidth: Some(CpuBandwidthStat::expected([4, 5, 6], None)),
        }),
      ),
      (
        indoc! {"
          usage_usec 0
          user_usec 0
          system_usec 0
          nr_periods 0
          nr_throttled 0
          throttled_usec 0
          nr_bursts 0
          burst_usec 0
        "},
        Ok(CpuStat {
          time: CpuTimeStat::expected([0, 0, 0]),
          bandwidth: Some(CpuBandwidthStat::expected([0, 0, 0], Some(CpuBurstStat::expected([0, 0])))),
        }),
      ),
      (
        indoc! {"
          system_usec 3
          future_counter nope
          usage_usec 1
          future_usec 4
          user_usec 2
        "},
        Ok(CpuStat { time: CpuTimeStat::expected([1, 2, 3]), bandwidth: None }),
      ),
      ("", Err(Missing("usage_usec"))),
      (
        indoc! {"
          usage_usec 1
          user_usec 2
        "},
        Err(Missing("system_usec")),
      ),
      (
        indoc! {"
          usage_usec nope
          user_usec 2
          system_usec 3
        "},
        Err(Invalid("usage_usec", "nope")),
      ),
      (
        indoc! {"
          usage_usec 1
          user_usec 2
          system_usec 3
          nr_periods 4
        "},
        Err(Missing("nr_throttled")),
      ),
      (
        indoc! {"
          usage_usec 18446744073709552
          user_usec 2
          system_usec 3
        "},
        Err(Invalid("usage_usec", "18446744073709552")),
      ),
      ("usage_usec 1\nuser_usec 2\nsystem_usec\n", Err(Missing("system_usec"))),
      (
        "usage_usec 1\nuser_usec 2\nsystem_usec 3\nnr_periods 4\nnr_throttled 5\nthrottled_usec 6\nnr_bursts 7\n",
        Err(Missing("burst_usec")),
      ),
    ]);
  }

  #[test]
  fn parses_stat_groups_independently() {
    let input = indoc! {"
      usage_usec 54321
      user_usec 32100
      system_usec 22221
      nr_periods 100
      nr_throttled 7
      throttled_usec 1234
      nr_bursts 3
      burst_usec 456
    "};
    let time = CpuTimeStat::expected([54_321, 32_100, 22_221]);
    let burst = CpuBurstStat::expected([3, 456]);
    let bandwidth = CpuBandwidthStat::expected([100, 7, 1_234], Some(burst));

    assert_eq!(assert_ok!(input.parse::<CpuTimeStat>()), time);
    assert_eq!(assert_ok!(input.parse::<CpuBandwidthStat>()), bandwidth);
    assert_eq!(assert_ok!(input.parse::<CpuBurstStat>()), burst);
  }

  #[test]
  fn preserves_optional_nice_and_core_scheduling_times() {
    Cases::<CpuTimeStat>::check([
      (
        "usage_usec 10\nuser_usec 7\nsystem_usec 3\nnice_usec 2\ncore_sched.force_idle_usec 0\n",
        Ok(CpuTimeStat {
          nice: Some(Time::new::<microsecond>(2)),
          force_idle: Some(Time::new::<microsecond>(0)),
          ..CpuTimeStat::expected([10, 7, 3])
        }),
      ),
      ("usage_usec 10\nuser_usec 7\nsystem_usec 3\nnice_usec nope\n", Err(Invalid("nice_usec", "nope"))),
      (
        "usage_usec 10\nuser_usec 7\nsystem_usec 3\ncore_sched.force_idle_usec 18446744073709552\n",
        Err(Invalid("core_sched.force_idle_usec", "18446744073709552")),
      ),
    ]);
  }

  #[test]
  fn preserves_wrapped_signed_bandwidth_counter_output() {
    Cases::<CpuBandwidthStat>::check([
      (
        "nr_periods -2147483648\nnr_throttled -1\nthrottled_usec 0\nnr_bursts -2147483647\nburst_usec 0\n",
        Ok(CpuBandwidthStat::expected(
          [2_147_483_648, 4_294_967_295, 0],
          Some(CpuBurstStat::expected([2_147_483_649, 0])),
        )),
      ),
      ("nr_periods -2147483649\nnr_throttled 0\nthrottled_usec 0", Err(Invalid("nr_periods", "-2147483649"))),
    ]);
  }
}
