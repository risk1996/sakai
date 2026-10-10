use std::str::FromStr;

use crate::{
  error::{ParseError, ParseValueError},
  parse::{ParseBoolean, Parser},
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
  /// The cgroup v2 `cpu.idle` interface filename.
  pub const FILE_NAME: &'static str = "cpu.idle";

  /// Returns whether idle scheduling is enabled.
  #[must_use]
  pub const fn value(self) -> bool { self.value }
}

impl FromStr for CpuIdle {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let value = Parser::single::<ParseBoolean, _>(contents, "idle")?;
    Ok(Self { value })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::parse::tests::Cases;

  #[test]
  fn parses_cpu_idle() { Cases::<CpuIdle>::boolean("idle", |value| CpuIdle { value }); }
}
