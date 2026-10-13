use std::{collections::BTreeMap, str::FromStr};

use crate::{
  error::{ParseError, ParseValueError},
  parse::{KeyedFields, ParseMicroseconds, ParsePercent},
  unit::{Ratio, Time},
};

/// Pressure stall information from a cgroup `*.pressure` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain fresh values. Averages are ratios over the preceding 10,
/// 60, and 300 seconds, using exponentially weighted averaging. Total stall times are reported by the kernel in
/// microseconds and stored as [`Time`].
///
/// CPU pressure files before Linux 5.13 contain only the `some` line, so
/// [`Self::full`] is optional. Reading pressure never requires writing a PSI
/// trigger to the file.
///
/// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pressure {
  some: PressureLine,
  full: Option<PressureLine>,
}

impl Pressure {
  /// The cgroup v2 `cpu.pressure` interface filename.
  ///
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  pub const CPU_FILE_NAME: &'static str = "cpu.pressure";
  /// The cgroup v2 `io.pressure` interface filename.
  ///
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  pub const IO_FILE_NAME: &'static str = "io.pressure";
  /// The cgroup v2 `memory.pressure` interface filename.
  ///
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  pub const MEMORY_FILE_NAME: &'static str = "memory.pressure";

  /// Returns pressure during which at least one task was stalled.
  ///
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  #[must_use]
  pub const fn some(&self) -> PressureLine { self.some }

  /// Returns pressure during which all non-idle tasks were stalled.
  ///
  /// CPU PSI before Linux 5.13 does not report this line. `None` means the
  /// line was absent; it does not mean zero stall time.
  ///
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  #[must_use]
  pub const fn full(&self) -> Option<PressureLine> { self.full }
}

/// A set of rolling averages and cumulative stall time from PSI.
///
/// Averages are exponentially weighted percentages parsed as [`Ratio`]; a
/// value of 0.25 represents 25% stalled time. Total time is accumulated since
/// accounting began, converted from kernel microseconds to [`Time`].
///
/// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PressureLine {
  avg10: Ratio,
  avg60: Ratio,
  avg300: Ratio,
  total: Time,
}

impl PressureLine {
  /// Returns the exponentially weighted stalled-time ratio with a 10-second averaging period.
  ///
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  #[must_use]
  pub const fn avg10(&self) -> Ratio { self.avg10 }

  /// Returns the exponentially weighted stalled-time ratio with a 60-second averaging period.
  ///
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  #[must_use]
  pub const fn avg60(&self) -> Ratio { self.avg60 }

  /// Returns the exponentially weighted stalled-time ratio with a 300-second averaging period.
  ///
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  #[must_use]
  pub const fn avg300(&self) -> Ratio { self.avg300 }

  /// Returns cumulative stall time, converted from kernel microseconds to [`Time`].
  ///
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  #[must_use]
  pub const fn total(&self) -> Time { self.total }

  fn from_fields(fields: &KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    Ok(Self {
      avg10: fields.required::<ParsePercent, _>(PressureField::Avg10)?,
      avg60: fields.required::<ParsePercent, _>(PressureField::Avg60)?,
      avg300: fields.required::<ParsePercent, _>(PressureField::Avg300)?,
      total: fields.required::<ParseMicroseconds, _>(PressureField::Total)?,
    })
  }
}

impl FromStr for Pressure {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let lines = parse_lines(contents)?;

    Ok(Self {
      some: lines.get(&PressureKind::Some).copied().ok_or_else(|| ParseError::missing(contents, "some"))?,
      full: lines.get(&PressureKind::Full).copied(),
    })
  }
}

/// Interrupt pressure from the cgroup v2 `irq.pressure` interface.
///
/// Unlike CPU, memory, and I/O pressure, interrupt pressure reports only a
/// `full` line: time during which interrupts prevented tasks from running.
/// Averages are ratios and cumulative stall time is stored as [`Time`]. This
/// snapshot does not register a pressure trigger; read again for fresh values.
///
/// Since Linux 6.1; availability requires PSI and `CONFIG_IRQ_TIME_ACCOUNTING`.
/// See the [Linux 6.1 PSI implementation](https://github.com/torvalds/linux/blob/v6.1/kernel/sched/psi.c)
/// and [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
/// Since Linux 6.14, reading with IRQ time accounting disabled at runtime
/// returns [`crate::Error::NotSupported`]; see the
/// [Linux 6.14 implementation](https://github.com/torvalds/linux/blob/v6.14/kernel/sched/psi.c).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IrqPressure {
  full: PressureLine,
}

impl IrqPressure {
  /// The cgroup v2 `irq.pressure` interface filename.
  ///
  /// Since Linux 6.1; see the [Linux 6.1 cgroup implementation](https://github.com/torvalds/linux/blob/v6.1/kernel/cgroup/cgroup.c).
  pub const FILE_NAME: &'static str = "irq.pressure";

  /// Returns interrupt pressure, the only line reported by this interface.
  ///
  /// See the [Linux interrupt PSI implementation](https://github.com/torvalds/linux/blob/v6.1/kernel/sched/psi.c).
  #[must_use]
  pub const fn full(&self) -> PressureLine { self.full }
}

impl FromStr for IrqPressure {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let lines = parse_lines(contents)?;
    Ok(Self { full: lines.get(&PressureKind::Full).copied().ok_or_else(|| ParseError::missing(contents, "full"))? })
  }
}

fn parse_lines(contents: &str) -> Result<BTreeMap<PressureKind, PressureLine>, ParseError<ParseValueError>> {
  contents
    .lines()
    .filter(|line| !line.trim().is_empty())
    .filter_map(|line| PressureKind::parse(contents, line).transpose())
    .map(|result| result.and_then(|(kind, fields)| Ok((kind, PressureLine::from_fields(&fields)?))))
    .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
enum PressureKind {
  Some,
  Full,
}

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::Display, strum::IntoStaticStr,
)]
#[strum(serialize_all = "lowercase")]
enum PressureField {
  Avg10,
  Avg60,
  Avg300,
  Total,
}

impl PressureKind {
  fn parse<'a>(raw: &'a str, line: &'a str) -> Result<Option<(Self, KeyedFields<'a>)>, ParseError<ParseValueError>> {
    let mut tokens = line.split_ascii_whitespace();
    let kind = tokens.next().ok_or_else(|| ParseError::missing(raw, "kind"))?;
    let Ok(kind) = Self::from_str(kind) else {
      return Ok(None);
    };
    let values = tokens.filter_map(|token| match token.split_once('=') {
      | Some((key, value)) => PressureField::from_str(key).ok().map(|_| Ok((key, value))),
      | None => Some(Err(ParseError::invalid(raw, "value", token, ParseValueError::OutOfRange))),
    });
    Ok(Some((kind, KeyedFields::new(raw, values)?)))
  }
}

#[cfg(test)]
mod tests {
  use indoc::indoc;
  use uom::si::{ratio::percent, time::microsecond};

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Invalid, Missing},
  };

  impl PressureLine {
    fn expected(averages: [f64; 3], total: u64) -> Self {
      let [avg10, avg60, avg300] = averages.map(Ratio::new::<percent>);
      Self { avg10, avg60, avg300, total: Time::new::<microsecond>(total) }
    }
  }

  #[test]
  fn parses_io_pressure_fixtures() {
    Cases::<Pressure>::check([
      (
        include_str!("cgroup/v2/fixtures/p3/io.pressure"),
        Ok(Pressure {
          some: PressureLine::expected([12.34, 5.67, 0.89], 1_234_567),
          full: Some(PressureLine::expected([1.25, 0.50, 0.10], 98_765)),
        }),
      ),
      (
        include_str!("cgroup/v2/fixtures/p3_older/io.pressure"),
        Ok(Pressure { some: PressureLine::expected([0.00, 0.01, 0.02], 42), full: None }),
      ),
    ]);
  }

  #[test]
  fn parses_cpu_pressure() {
    Cases::<Pressure>::check([
      (
        indoc! {"
          some avg10=12.34 avg60=5.67 avg300=0.89 total=1234567
          full avg10=1.25 avg60=0.50 avg300=0.10 total=98765
        "},
        Ok(Pressure {
          some: PressureLine::expected([12.34, 5.67, 0.89], 1_234_567),
          full: Some(PressureLine::expected([1.25, 0.50, 0.10], 98_765)),
        }),
      ),
      (
        "some avg10=0.00 avg60=0.01 avg300=0.02 total=42\n",
        Ok(Pressure { some: PressureLine::expected([0.00, 0.01, 0.02], 42), full: None }),
      ),
      (
        indoc! {"
          future avg10=99.00
          some total=7 future=99 avg300=3.00 avg10=1.00 avg60=2.00
        "},
        Ok(Pressure { some: PressureLine::expected([1.00, 2.00, 3.00], 7), full: None }),
      ),
      ("some avg10=1.00 avg60=2.00 total=4\n", Err(Missing("avg300"))),
      ("some avg10=nope avg60=0.00 avg300=0.00 total=0\n", Err(Invalid("avg10", "nope"))),
      ("some avg10=NaN avg60=0.00 avg300=0.00 total=0\n", Err(Invalid("avg10", "NaN"))),
      ("some avg10=-0.01 avg60=0.00 avg300=0.00 total=0\n", Err(Invalid("avg10", "-0.01"))),
      ("some avg10=100.01 avg60=0.00 avg300=0.00 total=0\n", Err(Invalid("avg10", "100.01"))),
      ("some avg10=0.00 avg60=0.00 avg300=0.00 total=18446744073709552\n", Err(Invalid("total", "18446744073709552"))),
      ("some avg10=0.00 avg60=0.00 avg300=0.00 total=1 malformed\n", Err(Invalid("value", "malformed"))),
    ]);
  }

  #[test]
  fn parses_full_only_irq_pressure() {
    Cases::<IrqPressure>::check([
      (
        "full avg10=12.50 avg60=1.00 avg300=0.00 total=42\n",
        Ok(IrqPressure { full: PressureLine::expected([12.50, 1.00, 0.00], 42) }),
      ),
      (
        "future avg10=nope\nfull total=0 avg300=0.00 avg60=0.00 avg10=0.00\n",
        Ok(IrqPressure { full: PressureLine::expected([0.00, 0.00, 0.00], 0) }),
      ),
      ("", Err(Missing("full"))),
      ("some avg10=0.00 avg60=0.00 avg300=0.00 total=0\n", Err(Missing("full"))),
      ("full avg10=0.00 avg60=0.00 total=0\n", Err(Missing("avg300"))),
      ("full avg10=nope avg60=0.00 avg300=0.00 total=0\n", Err(Invalid("avg10", "nope"))),
      ("full avg10=0.00 avg60=0.00 avg300=0.00 total=-1\n", Err(Invalid("total", "-1"))),
    ]);
  }
}
