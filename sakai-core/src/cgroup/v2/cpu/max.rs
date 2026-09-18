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
    let mut iters = Parser::new(contents.trim());
    let quota = iters.next_field::<ParseMicroseconds, _>("quota")?;
    let period = iters.next_field::<ParseMicroseconds, _>("period")?;
    iters.finish()?;

    Ok(Self { quota, period })
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
      expected: Result<(MaxOr<u64>, u64), &'static str>,
    }

    let cases = [
      TestCase {
        input: "25000 100000\n",
        expected: Ok((MaxOr::Value(25_000), 100_000)),
      },
      TestCase {
        input: "max 100000\n",
        expected: Ok((MaxOr::Max, 100_000)),
      },
      TestCase {
        input: "1500001 100000\n",
        expected: Ok((MaxOr::Value(1_500_001), 100_000)),
      },
      TestCase {
        input: "18446744073709551 100000\n",
        expected: Ok((MaxOr::Value(u64::MAX / 1_000), 100_000)),
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
    ];

    for case in cases {
      let actual = case.input.parse::<CpuMax>();
      match case.expected {
        | Ok((quota, period)) => {
          let actual = assert_ok!(actual);
          let actual_quota = match actual.quota() {
            | MaxOr::Max => MaxOr::Max,
            | MaxOr::Value(value) => MaxOr::Value(value.get::<microsecond>()),
          };
          assert_eq!(actual_quota, quota, "input: {:?}", case.input);
          assert_eq!(
            actual.period().get::<microsecond>(),
            period,
            "input: {:?}",
            case.input
          );
        },
        | Err(message) => {
          let actual = assert_err!(actual);
          assert_eq!(actual.to_string(), message, "input: {:?}", case.input);
        },
      }
    }
  }
}
