//! Limit bindings and conversions.

use pyo3::{exceptions::PyValueError, prelude::*};
use sakai::{Bytes, Count, MaxOr as CoreMaxOr, NonZeroTime, Ratio};
use uom::si::{information::byte, ratio::ratio, time::nanosecond};

#[derive(Debug, Clone, Copy, pyo3::IntoPyObject)]
pub(super) enum PythonLimitValue {
  Integer(u64),
  Ratio(f64),
}

/// A kernel `max` or a concrete value in the surrounding field's units.
///
/// This class is generic in Python: integer limits use `MaxOr[int]` and
/// dimensionless ratios use `MaxOr[float]`. Check `is_max` before reading
/// `value`; accessing `value` for `max` raises ValueError.
#[pyclass(generic, frozen, skip_from_py_object, module = "sakai._sakai")]
#[derive(Clone, Copy)]
pub(super) struct MaxOr {
  inner: Option<PythonLimitValue>,
}

impl MaxOr {
  pub(super) fn from_value<T>(value: CoreMaxOr<T>, convert: impl FnOnce(T) -> PythonLimitValue) -> Self {
    Self {
      inner: match value {
        | CoreMaxOr::Max => None,
        | CoreMaxOr::Value(value) => Some(convert(value)),
      },
    }
  }

  pub(super) fn from_time(value: CoreMaxOr<NonZeroTime>) -> Self {
    Self::from_value(value, |time| PythonLimitValue::Integer(time.as_ref().get::<nanosecond>()))
  }

  pub(super) fn from_bytes(value: CoreMaxOr<Bytes>) -> Self {
    Self::from_value(value, |bytes| PythonLimitValue::Integer(bytes.get::<byte>()))
  }

  pub(super) fn from_ratio(value: CoreMaxOr<Ratio>) -> Self {
    Self::from_value(value, |value| PythonLimitValue::Ratio(value.get::<ratio>()))
  }

  pub(super) fn from_count(value: CoreMaxOr<Count>) -> Self {
    Self::from_value(value, |count| PythonLimitValue::Integer(count.value))
  }
}

#[pymethods]
impl MaxOr {
  /// Whether the kernel reported `max`.
  #[getter]
  pub(super) fn is_max(&self) -> bool { self.inner.is_none() }

  /// Concrete value; raises ValueError when the kernel reported `max`.
  #[getter]
  pub(super) fn value(&self) -> PyResult<PythonLimitValue> {
    self.inner.ok_or_else(|| PyValueError::new_err("max has no numeric value"))
  }

  fn __repr__(&self) -> String {
    match self.inner {
      | None => "MaxOr(max)".into(),
      | Some(PythonLimitValue::Integer(value)) => format!("MaxOr(value={value})"),
      | Some(PythonLimitValue::Ratio(value)) => format!("MaxOr(value={value})"),
    }
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_in_delta, assert_ok};
  use pyo3::types::{PyFloat, PyInt};

  use super::*;

  #[test]
  fn max_or_exposes_typed_python_values_and_guards_max() {
    Python::initialize();
    Python::attach(|py| {
      let integer = assert_ok!(Py::new(py, MaxOr::from_bytes(CoreMaxOr::Value(Bytes::new::<byte>(0)))));
      let integer_value = assert_ok!(integer.bind(py).getattr("value"));
      assert!(integer_value.is_instance_of::<PyInt>());
      assert_eq!(assert_ok!(integer_value.extract::<u64>()), 0);

      let ratio_limit = assert_ok!(Py::new(py, MaxOr::from_ratio(CoreMaxOr::Value(Ratio::new::<ratio>(0.25)))));
      let ratio_value = assert_ok!(ratio_limit.bind(py).getattr("value"));
      assert!(ratio_value.is_instance_of::<PyFloat>());
      assert_in_delta!(assert_ok!(ratio_value.extract::<f64>()), 0.25, f64::EPSILON);

      let max = assert_ok!(Py::new(py, MaxOr::from_bytes(CoreMaxOr::Max)));
      let error = assert_err!(max.bind(py).getattr("value"));
      assert!(error.is_instance_of::<PyValueError>(py));
    });
  }
}
