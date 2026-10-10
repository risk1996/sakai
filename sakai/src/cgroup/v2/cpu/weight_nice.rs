use std::str::FromStr;

use nutype::nutype;

use crate::{
  error::{ParseError, ParseValueError},
  parse::{ParseCgroup, Parser},
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

impl Nice {
  /// The cgroup v2 `cpu.weight.nice` interface filename.
  pub const FILE_NAME: &'static str = "cpu.weight.nice";
}

impl FromStr for Nice {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| parser.next_field::<ParseNice, _>("nice"))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ParseNice;

impl ParseCgroup<ParseNice> for Nice {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    Self::try_new(value.parse::<i8>()?).map_err(|_error| ParseValueError::OutOfRange)
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
  fn parses_cpu_weight_nice() {
    let nice_minus_20 = assert_ok!(Nice::try_new(-20));
    let nice_0 = assert_ok!(Nice::try_new(0));
    let nice_19 = assert_ok!(Nice::try_new(19));
    Cases::<Nice>::check([
      ("-20\n", Ok(nice_minus_20)),
      ("0\n", Ok(nice_0)),
      ("19", Ok(nice_19)),
      ("", Err(Missing("nice"))),
      ("0 1", Err(Excess)),
      ("-21", Err(Invalid("nice", "-21"))),
      ("20", Err(Invalid("nice", "20"))),
      ("neutral", Err(Invalid("nice", "neutral"))),
    ]);
  }
}
