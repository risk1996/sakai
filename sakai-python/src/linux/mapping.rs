//! Read-only Python mappings built from stored snapshot values.

use pyo3::{
  prelude::*,
  types::{PyDict, PyMappingProxy},
};

pub(super) fn mapping<'py, K, V, O>(
  py: Python<'py>,
  values: impl IntoIterator<Item = (K, V)>,
  convert: impl Fn(V) -> PyResult<O>,
) -> PyResult<Py<PyMappingProxy>>
where
  K: IntoPyObject<'py>,
  O: IntoPyObject<'py>, {
  let dict = PyDict::new(py);
  for (key, value) in values {
    dict.set_item(key, convert(value)?)?;
  }
  Ok(PyMappingProxy::new(py, dict.as_mapping()).unbind())
}
