use std::str::FromStr;

use nutype::nutype;

use crate::cgroup::common::{
  error::ParseError,
  parser::{ParseCgroup, ParseValueError, Parser},
};

/// A CPU scheduling weight in the kernel-supported range.
#[nutype(
  validate(greater_or_equal = 1, less_or_equal = 10_000),
  derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRef, Deref)
)]
pub struct Weight(u16);

/// The CPU scheduling weight configured by a cgroup's `cpu.weight` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value. Idle cgroups use a distinct scheduler mode
/// instead of participating with a share weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CpuWeight {
  /// The cgroup uses idle scheduling.
  Idle,
  /// The cgroup participates with a weight from 1 through 10,000.
  Shares(Weight),
}

impl FromStr for CpuWeight {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      parser.next_field::<ParseCpuWeight, _>("weight")
    })
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ParseCpuWeight;

impl ParseCgroup<ParseCpuWeight> for CpuWeight {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    match value.parse::<u16>()? {
      | 0 => Ok(Self::Idle),
      | value => Weight::try_new(value)
        .map(Self::Shares)
        .map_err(|_error| ParseValueError::OutOfRange),
    }
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;

  #[test]
  fn parses_cpu_weight() {
    struct TestCase {
      input: &'static str,
      expected: Result<CpuWeight, &'static str>,
    }

    let weight_1 = assert_ok!(Weight::try_new(1));
    let weight_100 = assert_ok!(Weight::try_new(100));
    let weight_10_000 = assert_ok!(Weight::try_new(10_000));
    let cases = [
      TestCase {
        input: "0\n",
        expected: Ok(CpuWeight::Idle),
      },
      TestCase {
        input: "1",
        expected: Ok(CpuWeight::Shares(weight_1)),
      },
      TestCase {
        input: "100\n",
        expected: Ok(CpuWeight::Shares(weight_100)),
      },
      TestCase {
        input: "10000\n",
        expected: Ok(CpuWeight::Shares(weight_10_000)),
      },
      TestCase {
        input: "",
        expected: Err("cgroup content \"\" has missing field \"weight\""),
      },
      TestCase {
        input: "100 200",
        expected: Err(
          "cgroup content \"100 200\" has excess field \"additional\"",
        ),
      },
      TestCase {
        input: "10001",
        expected: Err(
          "cgroup content \"10001\" has an invalid field \"weight\" value \
           \"10001\"",
        ),
      },
      TestCase {
        input: "-1",
        expected: Err(
          "cgroup content \"-1\" has an invalid field \"weight\" value \"-1\"",
        ),
      },
      TestCase {
        input: "heavy",
        expected: Err(
          "cgroup content \"heavy\" has an invalid field \"weight\" value \
           \"heavy\"",
        ),
      },
    ];

    for case in cases {
      let actual = case.input.parse::<CpuWeight>();
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
  fn parses_live_root_cpu_weight_when_available() {
    let path = "/sys/fs/cgroup/cpu.weight";
    let contents = match std::fs::read_to_string(path) {
      | Ok(contents) => contents,
      | Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
      | Err(error) => panic!("failed to read {path}: {error}"),
    };

    assert_ok!(contents.parse::<CpuWeight>());
  }
}
