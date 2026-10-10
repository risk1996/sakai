use std::str::FromStr;

use crate::{
  error::{ParseError, ParseValueError},
  parse::{KeyedFields, ParseCount, ParseMicroseconds},
  unit::{Count, Time},
};

/// CPU usage counters from a cgroup's `cpu.stat` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain fresh values. All durations are reported by the kernel in
/// microseconds and stored as [`Time`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CpuStat {
  time: CpuTimeStat,
  bandwidth: Option<CpuBandwidthStat>,
}

impl CpuStat {
  /// The cgroup v2 `cpu.stat` interface filename.
  pub const FILE_NAME: &'static str = "cpu.stat";

  /// Returns CPU usage times.
  #[must_use]
  pub const fn time(&self) -> CpuTimeStat { self.time }

  /// Returns CPU bandwidth counters when the CPU controller reports them.
  #[must_use]
  pub const fn bandwidth(&self) -> Option<CpuBandwidthStat> { self.bandwidth }
}

/// Runqueue throttling in `cpu.stat.local`, including ancestor bandwidth limits.
///
/// This is a live snapshot. An empty file means the controller reports no
/// local counter; an absent file is handled separately by the reader.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CpuStatLocal {
  throttled: Option<Time>,
}

impl CpuStatLocal {
  /// The cgroup v2 `cpu.stat.local` interface filename.
  pub const FILE_NAME: &'static str = "cpu.stat.local";

  /// Returns throttling of this cgroup's own runqueues, when reported.
  pub const fn throttled(&self) -> Option<Time> { self.throttled }
}

impl FromStr for CpuStatLocal {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = CpuStatField::fields(contents)?;
    Ok(Self { throttled: fields.optional::<ParseMicroseconds, Time>("throttled_usec")? })
  }
}

/// CPU usage times from a cgroup's `cpu.stat` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpuTimeStat {
  usage: Time,
  user: Time,
  system: Time,
}

impl CpuTimeStat {
  /// Returns total CPU time consumed by the cgroup and its descendants.
  #[must_use]
  pub const fn usage(self) -> Time { self.usage }

  /// Returns CPU time consumed in user mode.
  #[must_use]
  pub const fn user(self) -> Time { self.user }

  /// Returns CPU time consumed in kernel mode.
  #[must_use]
  pub const fn system(self) -> Time { self.system }

  fn from_fields(fields: &KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    Ok(Self {
      usage: fields.required::<ParseMicroseconds, _>(CpuStatField::UsageUsec)?,
      user: fields.required::<ParseMicroseconds, _>(CpuStatField::UserUsec)?,
      system: fields.required::<ParseMicroseconds, _>(CpuStatField::SystemUsec)?,
    })
  }
}

/// CPU bandwidth counters from a cgroup's `cpu.stat` file.
/// These are non-hierarchical: ancestor-imposed throttling is excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpuBandwidthStat {
  nr_periods: Count,
  nr_throttled: Count,
  throttled: Time,
  burst: Option<CpuBurstStat>,
}

impl CpuBandwidthStat {
  /// Returns the number of elapsed enforcement periods.
  #[must_use]
  pub const fn nr_periods(self) -> Count { self.nr_periods }

  /// Returns the number of periods in which the cgroup was throttled.
  #[must_use]
  pub const fn nr_throttled(self) -> Count { self.nr_throttled }

  /// Returns the total time for which the cgroup was throttled.
  #[must_use]
  pub const fn throttled(self) -> Time { self.throttled }

  /// Returns CPU burst counters when reported by the kernel.
  #[must_use]
  pub const fn burst(self) -> Option<CpuBurstStat> { self.burst }

  fn from_fields(fields: &KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    let burst = if fields.contains_any(CpuStatField::bandwidth_burst_fields()) {
      Some(CpuBurstStat::from_fields(fields)?)
    } else {
      None
    };

    Ok(Self {
      nr_periods: fields.required::<ParseCount, _>(CpuStatField::NrPeriods)?,
      nr_throttled: fields.required::<ParseCount, _>(CpuStatField::NrThrottled)?,
      throttled: fields.required::<ParseMicroseconds, _>(CpuStatField::ThrottledUsec)?,
      burst,
    })
  }
}

/// CPU burst counters from a cgroup's `cpu.stat` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpuBurstStat {
  nr_bursts: Count,
  burst: Time,
}

impl CpuBurstStat {
  /// Returns the number of periods in which a burst occurred.
  #[must_use]
  pub const fn nr_bursts(self) -> Count { self.nr_bursts }

  /// Returns cumulative CPU time consumed above quota during bursts.
  #[must_use]
  pub const fn burst(self) -> Time { self.burst }

  fn from_fields(fields: &KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    Ok(Self {
      nr_bursts: fields.required::<ParseCount, _>(CpuStatField::NrBursts)?,
      burst: fields.required::<ParseMicroseconds, _>(CpuStatField::BurstUsec)?,
    })
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

impl FromStr for CpuTimeStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> { Self::from_fields(&CpuStatField::fields(contents)?) }
}

impl FromStr for CpuBandwidthStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> { Self::from_fields(&CpuStatField::fields(contents)?) }
}

impl FromStr for CpuBurstStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> { Self::from_fields(&CpuStatField::fields(contents)?) }
}

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::Display, strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum CpuStatField {
  UsageUsec,
  UserUsec,
  SystemUsec,
  NrPeriods,
  NrThrottled,
  ThrottledUsec,
  NrBursts,
  BurstUsec,
}

impl CpuStatField {
  fn fields(raw: &str) -> Result<KeyedFields<'_>, ParseError<ParseValueError>> {
    KeyedFields::parse(raw, |key| Self::from_str(key).map(Into::into).unwrap_or("value"))
  }

  pub fn bandwidth_fields() -> [Self; 5] {
    [Self::NrPeriods, Self::NrThrottled, Self::ThrottledUsec, Self::NrBursts, Self::BurstUsec]
  }

  pub fn bandwidth_burst_fields() -> [Self; 2] { [Self::NrBursts, Self::BurstUsec] }
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
          nice_usec 4
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
}
