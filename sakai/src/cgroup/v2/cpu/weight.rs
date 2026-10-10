use std::str::FromStr;

use nutype::nutype;

use crate::{
  error::{ParseError, ParseValueError},
  parse::{ParseCgroup, Parser},
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

impl CpuWeight {
  /// The cgroup v2 `cpu.weight` interface filename.
  pub const FILE_NAME: &'static str = "cpu.weight";
}

impl FromStr for CpuWeight {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| parser.next_field::<ParseCpuWeight, _>("weight"))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ParseCpuWeight;

impl ParseCgroup<ParseCpuWeight> for CpuWeight {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    match value.parse::<u16>()? {
      | 0 => Ok(Self::Idle),
      | value => Weight::try_new(value).map(Self::Shares).map_err(|_error| ParseValueError::OutOfRange),
    }
  }
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

  #[test]
  fn parses_cpu_weight() {
    let weight_1 = assert_ok!(Weight::try_new(1));
    let weight_100 = assert_ok!(Weight::try_new(100));
    let weight_10_000 = assert_ok!(Weight::try_new(10_000));
    Cases::<CpuWeight>::check([
      ("0\n", Ok(CpuWeight::Idle)),
      ("1", Ok(CpuWeight::Shares(weight_1))),
      ("100\n", Ok(CpuWeight::Shares(weight_100))),
      ("10000\n", Ok(CpuWeight::Shares(weight_10_000))),
      ("", Err(Missing("weight"))),
      ("100 200", Err(Excess)),
      ("10001", Err(Invalid("weight", "10001"))),
      ("-1", Err(Invalid("weight", "-1"))),
      ("heavy", Err(Invalid("weight", "heavy"))),
    ]);
  }
}
