//! Pressure bindings and conversions.

use pyo3::prelude::*;
use sakai::{IrqPressure as CoreIrqPressure, Pressure as CorePressure, PressureLine as CorePressureLine};

use crate::quantity::Quantity;

/// Pressure-stall information with some and optional full lines.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Pressure {
  /// Pressure during which at least one task was stalled.
  some: PressureLine,
  /// Pressure during which all non-idle tasks were stalled, if reported.
  full: Option<PressureLine>,
}

/// Interrupt pressure with its single full line; requires Linux 6.1 and IRQ time accounting.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct IrqPressure {
  /// Pressure while interrupts prevented tasks from running.
  full: PressureLine,
}

impl From<CoreIrqPressure> for IrqPressure {
  fn from(value: CoreIrqPressure) -> Self { Self { full: value.full().into() } }
}

impl From<CorePressure> for Pressure {
  fn from(value: CorePressure) -> Self { Self { some: value.some().into(), full: value.full().map(Into::into) } }
}

/// Pressure averages as ratios and cumulative stall time in nanoseconds.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PressureLine {
  /// Exponentially weighted ten-second pressure average, from zero to one.
  avg10: Quantity<sakai::Ratio>,
  /// Exponentially weighted sixty-second pressure average, from zero to one.
  avg60: Quantity<sakai::Ratio>,
  /// Exponentially weighted three-hundred-second pressure average, from zero to one.
  avg300: Quantity<sakai::Ratio>,
  /// Cumulative stall time in nanoseconds.
  total: Quantity<sakai::Time>,
}

impl From<CorePressureLine> for PressureLine {
  fn from(value: CorePressureLine) -> Self {
    Self {
      avg10: value.avg10().into(),
      avg60: value.avg60().into(),
      avg300: value.avg300().into(),
      total: value.total().into(),
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
    assert_in_delta!(pressure.some.avg10.0.value, 0.125, f64::EPSILON);
    assert_eq!(pressure.some.total, 23_000.into());
    assert!(pressure.full.is_none());
  }

  #[test]
  fn converts_full_only_irq_pressure() {
    let pressure =
      IrqPressure::from(assert_ok!("full avg10=12.50 avg60=0.00 avg300=100.00 total=23\n".parse::<CoreIrqPressure>()));
    assert_in_delta!(pressure.full.avg10.0.value, 0.125, f64::EPSILON);
    assert_eq!(pressure.full.total, 23_000.into());
  }
}
