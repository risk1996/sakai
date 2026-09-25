use std::str::FromStr;

use nutype::nutype;

use crate::cgroup::common::{
  error::ParseError,
  parser::{ParseCgroup, ParseValueError, Parser},
};

/// A CPU scheduling nice value in the kernel-supported range.
///
/// This is a point-in-time read of a cgroup's `cpu.weight.nice` file. Read the
/// file again to obtain a fresh value. It is a coarser representation of
/// `cpu.weight`, so a read returns the closest nice value for the current CPU
/// weight.
#[nutype(
  validate(greater_or_equal = -20, less_or_equal = 19),
  derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRef, Deref)
)]
pub struct Nice(i8);

impl FromStr for Nice {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| parser.next_field::<ParseNice, _>("nice"))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ParseNice;

impl ParseCgroup<ParseNice> for Nice {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    Self::try_new(value.parse::<i8>()?)
      .map_err(|_error| ParseValueError::OutOfRange)
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;

  #[test]
  fn parses_cpu_weight_nice() {
    struct TestCase {
      input: &'static str,
      expected: Result<Nice, &'static str>,
    }

    let nice_minus_20 = assert_ok!(Nice::try_new(-20));
    let nice_0 = assert_ok!(Nice::try_new(0));
    let nice_19 = assert_ok!(Nice::try_new(19));
    let cases = [
      TestCase {
        input: "-20\n",
        expected: Ok(nice_minus_20),
      },
      TestCase {
        input: "0\n",
        expected: Ok(nice_0),
      },
      TestCase {
        input: "19",
        expected: Ok(nice_19),
      },
      TestCase {
        input: "",
        expected: Err("cgroup content \"\" has missing field \"nice\""),
      },
      TestCase {
        input: "0 1",
        expected: Err("cgroup content \"0 1\" has excess field \"additional\""),
      },
      TestCase {
        input: "-21",
        expected: Err(
          "cgroup content \"-21\" has an invalid field \"nice\" value \"-21\"",
        ),
      },
      TestCase {
        input: "20",
        expected: Err(
          "cgroup content \"20\" has an invalid field \"nice\" value \"20\"",
        ),
      },
      TestCase {
        input: "neutral",
        expected: Err(
          "cgroup content \"neutral\" has an invalid field \"nice\" value \
           \"neutral\"",
        ),
      },
    ];

    for case in cases {
      let actual = case.input.parse::<Nice>();
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

  #[cfg(target_os = "linux")]
  #[test]
  fn parses_live_root_cpu_weight_nice_when_available() {
    let path = "/sys/fs/cgroup/cpu.weight.nice";
    let contents = match std::fs::read_to_string(path) {
      | Ok(contents) => contents,
      | Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
      | Err(error) => panic!("failed to read {path}: {error}"),
    };

    assert_ok!(contents.parse::<Nice>());
  }
}
