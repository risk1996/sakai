use std::str::FromStr;

use uom::si::{ratio::ratio, time::microsecond};

use crate::cgroup::common::{
  error::ParseError,
  parser::{ParseNonZeroMicroseconds, ParseValueError, Parser},
  unit::{MaxOr, NonZeroTime, Ratio},
};

/// The CPU bandwidth limit configured by a cgroup's `cpu.max` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpuMax {
  quota: MaxOr<NonZeroTime>,
  period: NonZeroTime,
}

impl CpuMax {
  /// Returns the configured CPU quota.
  #[must_use]
  pub const fn quota(self) -> MaxOr<NonZeroTime> {
    self.quota
  }

  /// Returns the quota period.
  #[must_use]
  pub const fn period(self) -> NonZeroTime {
    self.period
  }

  /// Returns the number of CPUs made available by the quota.
  ///
  /// An unlimited quota is returned as [`MaxOr::Max`].
  #[must_use]
  pub fn cpu_count(self) -> MaxOr<Ratio> {
    match self.quota {
      | MaxOr::Max => MaxOr::Max,
      #[expect(
        clippy::cast_precision_loss,
        reason = "valid kernel cpu.max values fit exactly in f64"
      )]
      | MaxOr::Value(quota) => {
        let quota = quota.get::<microsecond>() as f64;
        let period = self.period.get::<microsecond>() as f64;

        MaxOr::Value(Ratio::new::<ratio>(quota / period))
      },
    }
  }
}

impl FromStr for CpuMax {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        quota: parser.next_field::<ParseNonZeroMicroseconds, _>("quota")?,
        period: parser.next_field::<ParseNonZeroMicroseconds, _>("period")?,
      })
    })
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_in_delta, assert_ok};
  use uom::si::{ratio::ratio, time::microsecond};

  use super::*;
  use crate::cgroup::common::unit::Time;

  #[test]
  fn parses_cpu_max() {
    struct TestCase {
      input: &'static str,
      expected: Result<CpuMax, &'static str>,
    }

    let ms_25_000 =
      assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(25_000)));
    let ms_100_000 =
      assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(100_000)));
    let ms_1_500_001 =
      assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(1_500_001)));
    let ms_max = assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(
      u64::MAX / 1_000,
    )));
    let cases = [
      TestCase {
        input: "25000 100000\n",
        expected: Ok(CpuMax {
          quota: MaxOr::Value(ms_25_000),
          period: ms_100_000,
        }),
      },
      TestCase {
        input: "max 100000\n",
        expected: Ok(CpuMax {
          quota: MaxOr::Max,
          period: ms_100_000,
        }),
      },
      TestCase {
        input: "1500001 100000\n",
        expected: Ok(CpuMax {
          quota: MaxOr::Value(ms_1_500_001),
          period: ms_100_000,
        }),
      },
      TestCase {
        input: "18446744073709551 100000\n",
        expected: Ok(CpuMax {
          quota: MaxOr::Value(ms_max),
          period: ms_100_000,
        }),
      },
      TestCase {
        input: "",
        expected: Err("cgroup content \"\" has missing field \"quota\""),
      },
      TestCase {
        input: "max",
        expected: Err("cgroup content \"max\" has missing field \"period\""),
      },
      TestCase {
        input: "max 100000 extra",
        expected: Err(
          "cgroup content \"max 100000 extra\" has excess field \"additional\"",
        ),
      },
      TestCase {
        input: "unlimited 100000",
        expected: Err(
          "cgroup content \"unlimited 100000\" has an invalid field \"quota\" \
           value \"unlimited\"",
        ),
      },
      TestCase {
        input: "max forever",
        expected: Err(
          "cgroup content \"max forever\" has an invalid field \"period\" \
           value \"forever\"",
        ),
      },
      TestCase {
        input: "max max",
        expected: Err(
          "cgroup content \"max max\" has an invalid field \"period\" value \
           \"max\"",
        ),
      },
      TestCase {
        input: "25000 0",
        expected: Err(
          "cgroup content \"25000 0\" has an invalid field \"period\" value \
           \"0\"",
        ),
      },
      TestCase {
        input: "0 100000",
        expected: Err(
          "cgroup content \"0 100000\" has an invalid field \"quota\" value \
           \"0\"",
        ),
      },
      TestCase {
        input: " max forever\n",
        expected: Err(
          "cgroup content \" max forever\\n\" has an invalid field \"period\" \
           value \"forever\"",
        ),
      },
    ];

    for case in cases {
      let actual = case.input.parse::<CpuMax>();
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
  fn calculates_cpu_count() {
    let cases = [
      ("25000 100000", 0.25),
      ("100000 100000", 1.0),
      ("1500001 100000", 15.000_01),
      ("1 3", 1.0 / 3.0),
      ("17592186044415 1000", 17_592_186_044.415),
    ];

    for (input, expected) in cases {
      let cpu_max = assert_ok!(input.parse::<CpuMax>());
      let cpu_count = cpu_max.cpu_count();
      assert!(matches!(cpu_count, MaxOr::Value(_)));

      if let MaxOr::Value(cpu_count) = cpu_count {
        assert_in_delta!(cpu_count.get::<ratio>(), expected, f64::EPSILON);
      }
    }
  }

  #[test]
  fn preserves_unlimited_cpu_count() {
    let cpu_max = assert_ok!("max 100000".parse::<CpuMax>());

    assert!(matches!(cpu_max.cpu_count(), MaxOr::Max));
  }
}
