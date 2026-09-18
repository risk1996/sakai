use std::str::FromStr;

use crate::cgroup::common::{
  error::ParseError,
  parser::{ParseMicroseconds, ParseValueError, Parser},
  unit::{MaxOr, Time},
};

/// The CPU bandwidth limit configured by a cgroup's `cpu.max` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpuMax {
  quota: MaxOr<Time>,
  period: Time,
}

impl CpuMax {
  /// Returns the configured CPU quota.
  #[must_use]
  pub const fn quota(self) -> MaxOr<Time> {
    self.quota
  }

  /// Returns the quota period.
  #[must_use]
  pub const fn period(self) -> Time {
    self.period
  }
}

impl FromStr for CpuMax {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        quota: parser.next_field::<ParseMicroseconds, _>("quota")?,
        period: parser.next_field::<ParseMicroseconds, _>("period")?,
      })
    })
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use uom::si::time::microsecond;

  use super::*;

  #[test]
  fn parses_cpu_max() {
    struct TestCase {
      input: &'static str,
      expected: Result<CpuMax, &'static str>,
    }

    let cases = [
      TestCase {
        input: "25000 100000\n",
        expected: Ok(CpuMax {
          quota: MaxOr::Value(Time::new::<microsecond>(25_000)),
          period: Time::new::<microsecond>(100_000),
        }),
      },
      TestCase {
        input: "max 100000\n",
        expected: Ok(CpuMax {
          quota: MaxOr::Max,
          period: Time::new::<microsecond>(100_000),
        }),
      },
      TestCase {
        input: "1500001 100000\n",
        expected: Ok(CpuMax {
          quota: MaxOr::Value(Time::new::<microsecond>(1_500_001)),
          period: Time::new::<microsecond>(100_000),
        }),
      },
      TestCase {
        input: "18446744073709551 100000\n",
        expected: Ok(CpuMax {
          quota: MaxOr::Value(Time::new::<microsecond>(u64::MAX / 1_000)),
          period: Time::new::<microsecond>(100_000),
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
}
