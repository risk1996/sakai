//! Python bindings for the read-only Linux cgroup v2 API.

#[cfg(target_os = "linux")]
mod linux;

#[cfg(not(target_os = "linux"))]
use pyo3::{exceptions::PyImportError, prelude::*};

#[cfg(not(target_os = "linux"))]
#[pymodule]
fn _sakai(_module: &Bound<'_, PyModule>) -> PyResult<()> {
  Err(PyImportError::new_err("sakai requires Linux cgroup v2"))
}
