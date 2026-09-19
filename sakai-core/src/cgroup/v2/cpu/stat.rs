use std::{collections::BTreeMap, str::FromStr};

use crate::cgroup::common::{
  error::ParseError,
  parser::{
    ParseCgroup, ParseCount, ParseMicroseconds, ParseValueError, Parser,
  },
  unit::{Count, Time},
};

/// CPU usage counters from a cgroup's `cpu.stat` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain fresh values. All durations are reported by the kernel in
/// microseconds and stored as [`Time`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpuStat {
  time: CpuTimeStat,
  bandwidth: Option<CpuBandwidthStat>,
}

impl CpuStat {
  /// Returns CPU usage times.
  #[must_use]
  pub const fn time(self) -> CpuTimeStat {
    self.time
  }

  /// Returns CPU bandwidth counters when the CPU controller reports them.
  #[must_use]
  pub const fn bandwidth(self) -> Option<CpuBandwidthStat> {
    self.bandwidth
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
  pub const fn usage(self) -> Time {
    self.usage
  }

  /// Returns CPU time consumed in user mode.
  #[must_use]
  pub const fn user(self) -> Time {
    self.user
  }

  /// Returns CPU time consumed in kernel mode.
  #[must_use]
  pub const fn system(self) -> Time {
    self.system
  }

  fn from_fields(
    fields: &CpuStatFields<'_>,
  ) -> Result<Self, ParseError<'static, ParseValueError>> {
    Ok(Self {
      usage: fields
        .required::<ParseMicroseconds, _>(CpuStatField::UsageUsec)?,
      user: fields.required::<ParseMicroseconds, _>(CpuStatField::UserUsec)?,
      system: fields
        .required::<ParseMicroseconds, _>(CpuStatField::SystemUsec)?,
    })
  }
}

/// CPU bandwidth counters from a cgroup's `cpu.stat` file.
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
  pub const fn nr_periods(self) -> Count {
    self.nr_periods
  }

  /// Returns the number of periods in which the cgroup was throttled.
  #[must_use]
  pub const fn nr_throttled(self) -> Count {
    self.nr_throttled
  }

  /// Returns the total time for which the cgroup was throttled.
  #[must_use]
  pub const fn throttled(self) -> Time {
    self.throttled
  }

  /// Returns CPU burst counters when reported by the kernel.
  #[must_use]
  pub const fn burst(self) -> Option<CpuBurstStat> {
    self.burst
  }

  fn from_fields(
    fields: &CpuStatFields<'_>,
  ) -> Result<Self, ParseError<'static, ParseValueError>> {
    let burst = if fields.has_bandwidth_burst() {
      Some(CpuBurstStat::from_fields(fields)?)
    } else {
      None
    };

    Ok(Self {
      nr_periods: fields.required::<ParseCount, _>(CpuStatField::NrPeriods)?,
      nr_throttled: fields
        .required::<ParseCount, _>(CpuStatField::NrThrottled)?,
      throttled: fields
        .required::<ParseMicroseconds, _>(CpuStatField::ThrottledUsec)?,
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
  pub const fn nr_bursts(self) -> Count {
    self.nr_bursts
  }

  /// Returns cumulative CPU time consumed above quota during bursts.
  #[must_use]
  pub const fn burst(self) -> Time {
    self.burst
  }

  fn from_fields(
    fields: &CpuStatFields<'_>,
  ) -> Result<Self, ParseError<'static, ParseValueError>> {
    Ok(Self {
      nr_bursts: fields.required::<ParseCount, _>(CpuStatField::NrBursts)?,
      burst: fields
        .required::<ParseMicroseconds, _>(CpuStatField::BurstUsec)?,
    })
  }
}

impl FromStr for CpuStat {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = CpuStatFields::parse(contents)?;

    Ok(Self {
      time: CpuTimeStat::from_fields(&fields)?,
      bandwidth: if fields.has_bandwidth() {
        Some(CpuBandwidthStat::from_fields(&fields)?)
      } else {
        None
      },
    })
  }
}

impl FromStr for CpuTimeStat {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Self::from_fields(&CpuStatFields::parse(contents)?)
  }
}

impl FromStr for CpuBandwidthStat {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Self::from_fields(&CpuStatFields::parse(contents)?)
  }
}

impl FromStr for CpuBurstStat {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Self::from_fields(&CpuStatFields::parse(contents)?)
  }
}

#[derive(
  Debug,
  Clone,
  Copy,
  PartialEq,
  Eq,
  PartialOrd,
  Ord,
  Hash,
  strum::EnumString,
  strum::Display,
  strum::IntoStaticStr,
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
  pub fn bandwidth_fields() -> [Self; 5] {
    [
      Self::NrPeriods,
      Self::NrThrottled,
      Self::ThrottledUsec,
      Self::NrBursts,
      Self::BurstUsec,
    ]
  }

  pub fn bandwidth_burst_fields() -> [Self; 2] {
    [Self::NrBursts, Self::BurstUsec]
  }
}

struct CpuStatFields<'a> {
  raw: &'a str,
  values: BTreeMap<CpuStatField, &'a str>,
}

impl<'a> CpuStatFields<'a> {
  fn parse(raw: &'a str) -> Result<Self, ParseError<'static, ParseValueError>> {
    let values = raw
      .lines()
      .filter(|line| !line.trim().is_empty())
      .filter_map(|line| {
        Parser::parse_line(raw, line, |parser| {
          let key = parser.next_raw_field("key")?;
          let field = CpuStatField::from_str(key).ok();
          let name: &'static str = field.map(Into::into).unwrap_or("value");
          let value = parser.next_raw_field(name)?;

          Ok(field.map(|field| (field, value)))
        })
        .transpose()
      })
      .collect::<Result<_, _>>()?;

    Ok(Self { raw, values })
  }

  fn contains(&self, field: CpuStatField) -> bool {
    self.values.contains_key(&field)
  }

  fn has_bandwidth(&self) -> bool {
    CpuStatField::bandwidth_fields()
      .into_iter()
      .any(|field| self.contains(field))
  }

  fn has_bandwidth_burst(&self) -> bool {
    CpuStatField::bandwidth_burst_fields()
      .into_iter()
      .any(|field| self.contains(field))
  }

  fn required<Unit, T>(
    &self,
    field: CpuStatField,
  ) -> Result<T, ParseError<'static, T::Error>>
  where
    T: ParseCgroup<Unit>, {
    let name = field.into();
    let value = self
      .values
      .get(&field)
      .ok_or_else(|| ParseError::missing(self.raw, name))?;

    T::parse_field(self.raw, name, value)
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use indoc::indoc;
  use uom::si::time::microsecond;

  use super::*;

  #[test]
  fn parses_cpu_stat() {
    struct TestCase {
      input: &'static str,
      expected: Result<CpuStat, &'static str>,
    }

    let cases = [
      TestCase {
        input: indoc! {"
          usage_usec 54321
          user_usec 32100
          system_usec 22221
        "},
        expected: Ok(CpuStat {
          time: CpuTimeStat {
            usage: Time::new::<microsecond>(54_321),
            user: Time::new::<microsecond>(32_100),
            system: Time::new::<microsecond>(22_221),
          },
          bandwidth: None,
        }),
      },
      TestCase {
        input: indoc! {"
          usage_usec 54321
          user_usec 32100
          system_usec 22221
          nr_periods 100
          nr_throttled 7
          throttled_usec 1234
          nr_bursts 3
          burst_usec 456
        "},
        expected: Ok(CpuStat {
          time: CpuTimeStat {
            usage: Time::new::<microsecond>(54_321),
            user: Time::new::<microsecond>(32_100),
            system: Time::new::<microsecond>(22_221),
          },
          bandwidth: Some(CpuBandwidthStat {
            nr_periods: Count::new(100),
            nr_throttled: Count::new(7),
            throttled: Time::new::<microsecond>(1_234),
            burst: Some(CpuBurstStat {
              nr_bursts: Count::new(3),
              burst: Time::new::<microsecond>(456),
            }),
          }),
        }),
      },
      TestCase {
        input: indoc! {"
          usage_usec 1
          user_usec 2
          system_usec 3
          nr_periods 4
          nr_throttled 5
          throttled_usec 6
        "},
        expected: Ok(CpuStat {
          time: CpuTimeStat {
            usage: Time::new::<microsecond>(1),
            user: Time::new::<microsecond>(2),
            system: Time::new::<microsecond>(3),
          },
          bandwidth: Some(CpuBandwidthStat {
            nr_periods: Count::new(4),
            nr_throttled: Count::new(5),
            throttled: Time::new::<microsecond>(6),
            burst: None,
          }),
        }),
      },
      TestCase {
        input: indoc! {"
          usage_usec 0
          user_usec 0
          system_usec 0
          nr_periods 0
          nr_throttled 0
          throttled_usec 0
          nr_bursts 0
          burst_usec 0
        "},
        expected: Ok(CpuStat {
          time: CpuTimeStat {
            usage: Time::new::<microsecond>(0),
            user: Time::new::<microsecond>(0),
            system: Time::new::<microsecond>(0),
          },
          bandwidth: Some(CpuBandwidthStat {
            nr_periods: Count::new(0),
            nr_throttled: Count::new(0),
            throttled: Time::new::<microsecond>(0),
            burst: Some(CpuBurstStat {
              nr_bursts: Count::new(0),
              burst: Time::new::<microsecond>(0),
            }),
          }),
        }),
      },
      TestCase {
        input: indoc! {"
          system_usec 3
          future_counter 99
          usage_usec 1
          nice_usec 4
          user_usec 2
        "},
        expected: Ok(CpuStat {
          time: CpuTimeStat {
            usage: Time::new::<microsecond>(1),
            user: Time::new::<microsecond>(2),
            system: Time::new::<microsecond>(3),
          },
          bandwidth: None,
        }),
      },
      TestCase {
        input: "",
        expected: Err("cgroup content \"\" has missing field \"usage_usec\""),
      },
      TestCase {
        input: indoc! {"
          usage_usec 1
          user_usec 2
        "},
        expected: Err(
          "cgroup content \"usage_usec 1\\nuser_usec 2\\n\" has missing field \
           \"system_usec\"",
        ),
      },
      TestCase {
        input: indoc! {"
          usage_usec nope
          user_usec 2
          system_usec 3
        "},
        expected: Err(
          "cgroup content \"usage_usec nope\\nuser_usec 2\\nsystem_usec \
           3\\n\" has an invalid field \"usage_usec\" value \"nope\"",
        ),
      },
      TestCase {
        input: indoc! {"
          usage_usec 1
          user_usec 2
          system_usec 3
          nr_periods 4
        "},
        expected: Err(
          "cgroup content \"usage_usec 1\\nuser_usec 2\\nsystem_usec \
           3\\nnr_periods 4\\n\" has missing field \"nr_throttled\"",
        ),
      },
      TestCase {
        input: indoc! {"
          usage_usec 18446744073709552
          user_usec 2
          system_usec 3
        "},
        expected: Err(
          "cgroup content \"usage_usec 18446744073709552\\nuser_usec \
           2\\nsystem_usec 3\\n\" has an invalid field \"usage_usec\" value \
           \"18446744073709552\"",
        ),
      },
    ];

    for case in cases {
      let actual = case.input.parse::<CpuStat>();
      match case.expected {
        | Ok(expected) => {
          let actual = assert_ok!(actual, "input: {:?}", case.input);
          assert_eq!(actual, expected, "input: {:?}", case.input);
        },
        | Err(message) => {
          let actual = assert_err!(actual, "input: {:?}", case.input);
          assert_eq!(actual.to_string(), message, "input: {:?}", case.input);
        },
      }
    }
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
    let time = assert_ok!(input.parse::<CpuTimeStat>());
    let bandwidth = assert_ok!(input.parse::<CpuBandwidthStat>());
    let burst = assert_ok!(input.parse::<CpuBurstStat>());

    assert_eq!(time.usage().get::<microsecond>(), 54_321);
    assert_eq!(time.user().get::<microsecond>(), 32_100);
    assert_eq!(time.system().get::<microsecond>(), 22_221);
    assert_eq!(bandwidth.nr_periods().into_inner(), 100);
    assert_eq!(bandwidth.nr_throttled().into_inner(), 7);
    assert_eq!(bandwidth.throttled().get::<microsecond>(), 1_234);
    assert_eq!(bandwidth.burst(), Some(burst));
    assert_eq!(burst.nr_bursts().into_inner(), 3);
    assert_eq!(burst.burst().get::<microsecond>(), 456);
  }
}
