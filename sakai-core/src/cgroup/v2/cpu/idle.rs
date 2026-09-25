use std::str::FromStr;

use crate::cgroup::common::{
  error::ParseError,
  parser::{ParseBoolean, ParseValueError, Parser},
};

/// The idle scheduling state configured by a cgroup's `cpu.idle` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value. When enabled, the cgroup uses the idle
/// scheduling policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpuIdle {
  value: bool,
}

impl CpuIdle {
  /// Returns whether idle scheduling is enabled.
  #[must_use]
  pub const fn value(self) -> bool {
    self.value
  }
}

impl FromStr for CpuIdle {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBoolean, _>("idle")?,
      })
    })
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;

  #[test]
  fn parses_cpu_idle() {
    struct TestCase {
      input: &'static str,
      expected: Result<CpuIdle, &'static str>,
    }

    let cases = [
      TestCase {
        input: "0\n",
        expected: Ok(CpuIdle { value: false }),
      },
      TestCase {
        input: "1",
        expected: Ok(CpuIdle { value: true }),
      },
      TestCase {
        input: "",
        expected: Err("cgroup content \"\" has missing field \"idle\""),
      },
      TestCase {
        input: "1 0",
        expected: Err("cgroup content \"1 0\" has excess field \"additional\""),
      },
      TestCase {
        input: "2",
        expected: Err(
          "cgroup content \"2\" has an invalid field \"idle\" value \"2\"",
        ),
      },
      TestCase {
        input: "-1",
        expected: Err(
          "cgroup content \"-1\" has an invalid field \"idle\" value \"-1\"",
        ),
      },
      TestCase {
        input: "true",
        expected: Err(
          "cgroup content \"true\" has an invalid field \"idle\" value \
           \"true\"",
        ),
      },
    ];

    for case in cases {
      let actual = case.input.parse::<CpuIdle>();
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
