use std::str::FromStr;

use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{ParsePercent, Parser},
  unit::Ratio,
};

/// The minimum CPU utilization requested by a cgroup's `cpu.uclamp.min` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value. The kernel reports utilization as a decimal
/// percentage, which is exposed as a dimensionless [`Ratio`]. This request is
/// a performance hint rather than a CPU bandwidth guarantee.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuUclampMin {
  value: Ratio,
}

impl CpuUclampMin {
  /// The cgroup v2 `cpu.uclamp.min` interface filename.
  pub const FILE_NAME: &'static str = "cpu.uclamp.min";

  /// Returns the requested minimum CPU utilization.
  #[must_use]
  pub const fn value(self) -> Ratio { self.value }
}

impl FromStr for CpuUclampMin {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParsePercent, _>(contents, "utilization")?;
    Ok(Self { value })
  }
}

/// The maximum CPU utilization requested by a cgroup's `cpu.uclamp.max` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value. A concrete limit is reported as a decimal
/// percentage and exposed as a dimensionless [`Ratio`]. [`MaxOr::Max`] means
/// that the cgroup requests no utilization cap. This request is a performance
/// hint rather than a CPU bandwidth limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuUclampMax {
  value: MaxOr<Ratio>,
}

impl CpuUclampMax {
  /// The cgroup v2 `cpu.uclamp.max` interface filename.
  pub const FILE_NAME: &'static str = "cpu.uclamp.max";

  /// Returns the requested maximum CPU utilization.
  #[must_use]
  pub const fn value(self) -> MaxOr<Ratio> { self.value }
}

impl FromStr for CpuUclampMax {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParsePercent, _>(contents, "utilization")?;
    Ok(Self { value })
  }
}

#[cfg(test)]
mod tests {
  use uom::si::ratio::percent;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

  #[test]
  fn parses_cpu_uclamp() {
    let cases = [
      ("0.00\n", Ok(Ratio::new::<percent>(0.00))),
      ("12.34\n", Ok(Ratio::new::<percent>(12.34))),
      ("98.76\n", Ok(Ratio::new::<percent>(98.76))),
      ("100.00", Ok(Ratio::new::<percent>(100.00))),
      ("max", Err(Invalid("utilization", "max"))),
      ("", Err(Missing("utilization"))),
      ("12.34 extra", Err(Excess)),
      ("unlimited", Err(Invalid("utilization", "unlimited"))),
      ("-0.01", Err(Invalid("utilization", "-0.01"))),
      ("100.01", Err(Invalid("utilization", "100.01"))),
      ("NaN", Err(Invalid("utilization", "NaN"))),
    ];
    Cases::<CpuUclampMin>::check(cases.map(|(input, expected)| (input, expected.map(|value| CpuUclampMin { value }))));
    Cases::<CpuUclampMax>::check(cases.map(|(input, expected)| {
      let expected = match input {
        | "max" => Ok(CpuUclampMax { value: MaxOr::Max }),
        | _ => expected.map(|value| CpuUclampMax { value: MaxOr::Value(value) }),
      };
      (input, expected)
    }));
  }
}
