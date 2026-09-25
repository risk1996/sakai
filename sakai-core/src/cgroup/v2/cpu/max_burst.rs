use std::str::FromStr;

use crate::cgroup::common::{
  error::ParseError,
  parser::{ParseMicroseconds, ParseValueError, Parser},
  unit::Time,
};

/// The CPU bandwidth burst allowance configured by a cgroup's
/// `cpu.max.burst` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value. A zero duration disables bursting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpuMaxBurst {
  value: Time,
}

impl CpuMaxBurst {
  /// Returns the configured burst allowance.
  #[must_use]
  pub const fn value(self) -> Time {
    self.value
  }
}

impl FromStr for CpuMaxBurst {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseMicroseconds, _>("burst")?,
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
  fn parses_cpu_max_burst() {
    struct TestCase {
      input: &'static str,
      expected: Result<CpuMaxBurst, &'static str>,
    }

    let cases = [
      TestCase {
        input: "0\n",
        expected: Ok(CpuMaxBurst {
          value: Time::new::<microsecond>(0),
        }),
      },
      TestCase {
        input: "25000\n",
        expected: Ok(CpuMaxBurst {
          value: Time::new::<microsecond>(25_000),
        }),
      },
      TestCase {
        input: "18446744073709551",
        expected: Ok(CpuMaxBurst {
          value: Time::new::<microsecond>(u64::MAX / 1_000),
        }),
      },
      TestCase {
        input: "",
        expected: Err("cgroup content \"\" has missing field \"burst\""),
      },
      TestCase {
        input: "25000 extra",
        expected: Err(
          "cgroup content \"25000 extra\" has excess field \"additional\"",
        ),
      },
      TestCase {
        input: "max",
        expected: Err(
          "cgroup content \"max\" has an invalid field \"burst\" value \"max\"",
        ),
      },
      TestCase {
        input: "-1",
        expected: Err(
          "cgroup content \"-1\" has an invalid field \"burst\" value \"-1\"",
        ),
      },
      TestCase {
        input: "18446744073709552",
        expected: Err(
          "cgroup content \"18446744073709552\" has an invalid field \
           \"burst\" value \"18446744073709552\"",
        ),
      },
    ];

    for case in cases {
      let actual = case.input.parse::<CpuMaxBurst>();
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
