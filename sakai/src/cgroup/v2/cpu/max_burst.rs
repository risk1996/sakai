use std::str::FromStr;

use crate::{
  error::{ParseError, ParseValueError},
  parse::{ParseMicroseconds, Parser},
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
  /// The cgroup v2 `cpu.max.burst` interface filename.
  pub const FILE_NAME: &'static str = "cpu.max.burst";

  /// Returns the configured burst allowance.
  #[must_use]
  pub const fn value(self) -> Time { self.value }
}

impl FromStr for CpuMaxBurst {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParseMicroseconds, _>(contents, "burst")?;
    Ok(Self { value })
  }
}

#[cfg(test)]
mod tests {
  use uom::si::time::microsecond;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

  #[test]
  fn parses_cpu_max_burst() {
    Cases::<CpuMaxBurst>::check([
      ("0\n", Ok(CpuMaxBurst { value: Time::new::<microsecond>(0) })),
      ("25000\n", Ok(CpuMaxBurst { value: Time::new::<microsecond>(25_000) })),
      ("18446744073709551", Ok(CpuMaxBurst { value: Time::new::<microsecond>(u64::MAX / 1_000) })),
      ("", Err(Missing("burst"))),
      ("25000 extra", Err(Excess)),
      ("max", Err(Invalid("burst", "max"))),
      ("-1", Err(Invalid("burst", "-1"))),
      ("18446744073709552", Err(Invalid("burst", "18446744073709552"))),
    ]);
  }
}
