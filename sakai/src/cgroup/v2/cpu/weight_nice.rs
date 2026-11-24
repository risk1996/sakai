use std::fmt::Write;

use nutype::nutype;

use crate::{
  error::ParseValueError,
  parse::{EncodeCgroup, ParseCgroup, cgroup_value},
  v2::interface::cgroup_file,
};

/// A CPU scheduling nice value from -20 (highest weight) through 19 (lowest weight).
///
/// This is a point-in-time read of a cgroup's `cpu.weight.nice` file. Read the
/// file again to obtain a fresh value. It is a coarser representation of
/// `cpu.weight`, so a read returns the closest nice value for the current CPU
/// weight.
/// Use `try_new` to validate the range and `into_inner` to recover the integer.
/// The nice scale is described in [sched(7)](https://man7.org/linux/man-pages/man7/sched.7.html).
///
/// See [Linux `cpu.weight.nice` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
#[nutype(
  validate(with = Nice::validate_nice, error = NiceError),
  derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRef)
)]
#[non_exhaustive]
pub struct Nice(i8);

cgroup_value! { token Nice: ParseNice, "nice" }
cgroup_file!(Nice, "cpu.weight.nice", ReadWrite);

impl Nice {
  fn validate_nice(nice: &i8) -> Result<(), NiceError> {
    match *nice {
      | ..=-21 => Err(NiceError::GreaterOrEqualViolated),
      | -20..=19 => Ok(()),
      | _ => Err(NiceError::LessOrEqualViolated),
    }
  }
}

/// An invalid CPU scheduling [`Nice`] value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NiceError {
  /// The nice value is below -20.
  #[error("Nice is too small. The value must be greater or equal to -20.")]
  GreaterOrEqualViolated,
  /// The nice value is above 19.
  #[error("Nice is too big. The value must be less or equal to 19.")]
  LessOrEqualViolated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ParseNice;

impl ParseCgroup<ParseNice> for Nice {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    Self::try_new(value.parse::<i8>()?).map_err(|_error| ParseValueError::OutOfRange)
  }
}

impl EncodeCgroup<ParseNice> for Nice {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    let value = self.into_inner();
    write!(output, "{value}").ok()
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
