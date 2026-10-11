//! Pressure bindings and conversions.

use pyo3::prelude::*;
use sakai::{Pressure as CorePressure, PressureLine as CorePressureLine};
use uom::si::{ratio::ratio, time::nanosecond};

/// Pressure-stall information with some and optional full lines.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Pressure {
  /// Time during which some tasks were stalled.
  some: PressureLine,
  /// Time during which all tasks were stalled, if reported.
  full: Option<PressureLine>,
}

impl From<CorePressure> for Pressure {
  fn from(value: CorePressure) -> Self { Self { some: value.some().into(), full: value.full().map(Into::into) } }
}

/// Pressure averages as ratios and cumulative stall time in nanoseconds.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PressureLine {
  /// Ten-second pressure average, from zero to one.
  avg10: f64,
  /// Sixty-second pressure average, from zero to one.
  avg60: f64,
  /// Three-hundred-second pressure average, from zero to one.
  avg300: f64,
  /// Cumulative stall time in nanoseconds.
  total_ns: u64,
}

impl From<CorePressureLine> for PressureLine {
  fn from(value: CorePressureLine) -> Self {
    Self {
      avg10: value.avg10().get::<ratio>(),
      avg60: value.avg60().get::<ratio>(),
      avg300: value.avg300().get::<ratio>(),
      total_ns: value.total().get::<nanosecond>(),
    }
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_in_delta, assert_ok};

  use super::*;

  #[test]
  fn converts_pressure_percent_and_optional_full_line() {
    let pressure =
      Pressure::from(assert_ok!("some avg10=12.50 avg60=0.00 avg300=100.00 total=23\n".parse::<CorePressure>()));
    assert_in_delta!(pressure.some.avg10, 0.125, f64::EPSILON);
    assert_eq!(pressure.some.total_ns, 23_000);
    assert!(pressure.full.is_none());
  }
}
