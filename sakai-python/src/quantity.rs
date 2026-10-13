//! Conversion of typed Rust quantities into ordinary Pint quantities.

use pyo3::{prelude::*, types::PyModule};
use sakai::{Bytes, BytesPerSecond, Count, OperationsPerSecond, Pages, Ratio, Time};
use uom::si::{frequency::hertz, information::byte, information_rate::byte_per_second, ratio::ratio, time::nanosecond};

/// Retains a Rust quantity until Python requests a fresh Pint object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Quantity<Q>(pub(crate) Q);

impl<Q> From<Q> for Quantity<Q> {
  fn from(value: Q) -> Self { Self(value) }
}

impl<Q: Copy> From<&Q> for Quantity<Q> {
  fn from(value: &Q) -> Self { Self(*value) }
}

fn registry(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
  PyModule::import(py, "pint")?.call_method0("get_application_registry")
}

/// Finish Pint's lazy registry initialization while Python serializes imports.
pub(crate) fn initialize(py: Python<'_>) -> PyResult<()> {
  registry(py)?.getattr("Quantity")?;
  Ok(())
}

macro_rules! pint_quantity {
  ($quantity:ty, $magnitude:ty, $unit:literal, $value:ident => $extract:expr) => {
    impl<'py> IntoPyObject<'py> for Quantity<$quantity> {
      type Error = PyErr;
      type Output = Bound<'py, PyAny>;
      type Target = PyAny;

      fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> {
        let $value = self.0;
        registry(py)?.call_method1("Quantity", ($extract, $unit))
      }
    }

    impl<'py> IntoPyObject<'py> for &Quantity<$quantity> {
      type Error = PyErr;
      type Output = Bound<'py, PyAny>;
      type Target = PyAny;

      fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> { (*self).into_pyobject(py) }
    }

    #[cfg(feature = "stubs")]
    impl pyo3_stub_gen::PyStubType for Quantity<$quantity> {
      fn type_output() -> pyo3_stub_gen::TypeInfo {
        pyo3_stub_gen::TypeInfo::with_module(concat!("pint.Quantity[", stringify!($magnitude), "]"), "pint".into())
      }
    }
  };
}

pint_quantity!(Time, int, "nanosecond", value => value.get::<nanosecond>());
pint_quantity!(Bytes, int, "byte", value => value.get::<byte>());
pint_quantity!(Count, int, "count", value => value.value);
pint_quantity!(BytesPerSecond, int, "byte / second", value => value.get::<byte_per_second>());
pint_quantity!(OperationsPerSecond, int, "count / second", value => value.get::<hertz>());
pint_quantity!(Ratio, float, "dimensionless", value => value.get::<ratio>());

impl<'py> IntoPyObject<'py> for Quantity<Pages> {
  type Error = PyErr;
  type Output = Bound<'py, PyAny>;
  type Target = PyAny;

  fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> {
    let registry = registry(py)?;
    // Pages have no universal byte size. Add only our namespaced unit; never
    // redefine the application's standard information or count units.
    if !registry.contains("sakai_page")? {
      registry.call_method1("define", ("sakai_page = [sakai_page]",))?;
    }
    let unit = registry.call_method1("Quantity", (1, "sakai_page"))?;
    if !unit.call_method1("check", ("[sakai_page]",))?.extract::<bool>()?
      || !unit.call_method0("to_base_units")?.getattr("magnitude")?.eq(1)?
    {
      return Err(pyo3::exceptions::PyValueError::new_err("sakai_page must be an unscaled [sakai_page] unit"));
    }
    registry.call_method1("Quantity", (self.0.value, "sakai_page"))
  }
}

impl<'py> IntoPyObject<'py> for &Quantity<Pages> {
  type Error = PyErr;
  type Output = Bound<'py, PyAny>;
  type Target = PyAny;

  fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> { (*self).into_pyobject(py) }
}

#[cfg(feature = "stubs")]
impl pyo3_stub_gen::PyStubType for Quantity<Pages> {
  fn type_output() -> pyo3_stub_gen::TypeInfo {
    pyo3_stub_gen::TypeInfo::with_module("pint.Quantity[int]", "pint".into())
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::sync::Once;

  use assertables::{assert_err, assert_in_delta, assert_ok};

  use super::*;

  pub(crate) fn initialize_python() {
    static INITIALIZE: Once = Once::new();
    INITIALIZE.call_once(|| {
      Python::initialize();
      Python::attach(|py| assert_ok!(initialize(py)));
    });
  }

  macro_rules! test_quantity {
    ($quantity:ty, $magnitude:ty, $unit:literal, $value:ident => $construct:expr) => {
      impl From<$magnitude> for Quantity<$quantity> {
        fn from($value: $magnitude) -> Self { Self($construct) }
      }

      impl<'a, 'py> FromPyObject<'a, 'py> for Quantity<$quantity> {
        type Error = PyErr;

        fn extract(object: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
          Ok(object.call_method1("to", ($unit,))?.getattr("magnitude")?.extract::<$magnitude>()?.into())
        }
      }
    };
  }

  test_quantity!(Time, u64, "nanosecond", value => Time::new::<nanosecond>(value));
  test_quantity!(Bytes, u64, "byte", value => Bytes::new::<byte>(value));
  test_quantity!(Count, u64, "count", value => Count { value, ..Default::default() });
  test_quantity!(Pages, u64, "sakai_page", value => Pages { value, ..Default::default() });
  test_quantity!(BytesPerSecond, u64, "byte / second", value => BytesPerSecond::new::<byte_per_second>(value));
  test_quantity!(OperationsPerSecond, u64, "count / second", value => OperationsPerSecond::new::<hertz>(value));
  test_quantity!(Ratio, f64, "dimensionless", value => Ratio::new::<ratio>(value));

  #[test]
  fn preserves_integer_magnitudes_and_units() {
    initialize_python();
    Python::attach(|py| {
      for magnitude in [0, 1, (1_u64 << 53) + 1, u64::MAX] {
        let values = [
          assert_ok!(Quantity(Time::new::<nanosecond>(magnitude)).into_pyobject(py)),
          assert_ok!(Quantity(Bytes::new::<byte>(magnitude)).into_pyobject(py)),
          assert_ok!(Quantity(Count { value: magnitude, ..Default::default() }).into_pyobject(py)),
          assert_ok!(Quantity(Pages { value: magnitude, ..Default::default() }).into_pyobject(py)),
          assert_ok!(Quantity(BytesPerSecond::new::<byte_per_second>(magnitude)).into_pyobject(py)),
          assert_ok!(Quantity(OperationsPerSecond::new::<hertz>(magnitude)).into_pyobject(py)),
        ];
        for (value, unit) in
          values.into_iter().zip(["nanosecond", "byte", "count", "sakai_page", "byte / second", "count / second"])
        {
          assert_eq!(assert_ok!(assert_ok!(value.getattr("magnitude")).extract::<u64>()), magnitude);
          let expected = assert_ok!(assert_ok!(registry(py)).call_method1("Unit", (unit,)));
          assert!(assert_ok!(assert_ok!(value.getattr("units")).eq(expected)));
          assert_err!(value.call_method1("to", ("meter",)));
        }
      }
    });
  }

  #[test]
  fn retains_fractional_ratios_and_detaches_python_mutation() {
    initialize_python();
    Python::attach(|py| {
      let stored = Quantity(Ratio::new::<ratio>(0.125));
      let value = assert_ok!(stored.into_pyobject(py));
      assert_ok!(value.call_method1("ito", ("percent",)));
      assert_in_delta!(assert_ok!(assert_ok!(value.getattr("magnitude")).extract::<f64>()), 12.5, f64::EPSILON);
      let fresh = assert_ok!(stored.into_pyobject(py));
      assert_in_delta!(assert_ok!(assert_ok!(fresh.getattr("magnitude")).extract::<f64>()), 0.125, f64::EPSILON);
    });
  }
}
