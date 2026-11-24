//! Limit bindings and conversions.

use pyo3::{exceptions::PyValueError, prelude::*};
use sakai::{Bytes, BytesPerSecond, Count, MaxOr as CoreMaxOr, NonZeroTime, OperationsPerSecond, Ratio, Time};

use crate::quantity::Quantity;

#[derive(Debug, Clone, Copy, pyo3::IntoPyObject)]
pub(super) enum PythonLimitValue {
  Time(Quantity<Time>),
  Bytes(Quantity<Bytes>),
  Count(Quantity<Count>),
  Bandwidth(Quantity<BytesPerSecond>),
  Operations(Quantity<OperationsPerSecond>),
  Ratio(Quantity<Ratio>),
}

/// A kernel `max` or a concrete value in the surrounding field's units.
///
/// This class is generic in Python: limits contain native quantities.
/// Check `is_max` before reading
/// `value`; accessing `value` for `max` raises ValueError.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(generic, frozen, module = "sakai._sakai")]
#[derive(Clone, Copy)]
pub(super) struct MaxOr {
  inner: CoreMaxOr<PythonLimitValue>,
}

impl MaxOr {
  pub(super) fn from_value<T>(value: CoreMaxOr<T>, convert: impl FnOnce(T) -> PythonLimitValue) -> Self {
    Self { inner: value.map(convert) }
  }

  pub(super) fn from_time(value: CoreMaxOr<NonZeroTime>) -> Self {
    Self::from_value(value, |time| PythonLimitValue::Time(time.as_ref().into()))
  }

  pub(super) fn from_bytes(value: CoreMaxOr<Bytes>) -> Self {
    Self::from_value(value, |bytes| PythonLimitValue::Bytes(bytes.into()))
  }

  pub(super) fn from_ratio(value: CoreMaxOr<Ratio>) -> Self {
    Self::from_value(value, |value| PythonLimitValue::Ratio(value.into()))
  }

  pub(super) fn from_count(value: CoreMaxOr<Count>) -> Self {
    Self::from_value(value, |count| PythonLimitValue::Count(count.into()))
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl MaxOr {
  /// Whether the kernel reported `max`.
  #[getter]
  pub(super) fn is_max(&self) -> bool { matches!(self.inner, CoreMaxOr::Max) }

  /// Concrete value; raises ValueError when the kernel reported `max`.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "_ValueT"))]
  pub(super) fn value(&self) -> PyResult<PythonLimitValue> {
    match self.inner {
      | CoreMaxOr::Max => Err(PyValueError::new_err("max has no numeric value")),
      | CoreMaxOr::Value(value) => Ok(value),
      | _ => Err(PyValueError::new_err("unsupported cgroup limit")),
    }
  }

  fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
    if self.is_max() {
      return Ok("MaxOr(max)".into());
    }
    Ok(format!("MaxOr(value={})", self.value()?.into_pyobject(py)?.repr()?))
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_in_delta, assert_ok};
  use uom::si::{information::byte, ratio::ratio};

  use super::*;

  #[test]
  fn max_or_exposes_typed_python_values_and_guards_max() {
    Python::initialize();
    Python::attach(|py| {
      let integer = assert_ok!(Py::new(py, MaxOr::from_bytes(CoreMaxOr::Value(Bytes::new::<byte>(0)))));
      let integer_value = assert_ok!(integer.bind(py).getattr("value"));
      assert_eq!(assert_ok!(integer_value.extract::<Quantity<Bytes>>()), Quantity(Bytes::new::<byte>(0)));

      let ratio_limit = assert_ok!(Py::new(py, MaxOr::from_ratio(CoreMaxOr::Value(Ratio::new::<ratio>(0.25)))));
      let ratio_value = assert_ok!(ratio_limit.bind(py).getattr("value"));
      assert_in_delta!(assert_ok!(ratio_value.extract::<Quantity<Ratio>>()).0.value, 0.25, f64::EPSILON);

      let max = assert_ok!(Py::new(py, MaxOr::from_bytes(CoreMaxOr::Max)));
      let error = assert_err!(max.bind(py).getattr("value"));
      assert!(error.is_instance_of::<PyValueError>(py));
    });
  }
}
