//! Pids bindings and conversions.

use std::sync::Arc;

use pyo3::prelude::*;
use sakai::v2::pids::PidsEvents as CorePidsEvents;

use super::{MaxOr, PythonError};

/// Fresh process-number reads for one pinned cgroup. Counts include threads.
///
/// Separate calls are not atomic together. Missing files raise InterfaceMissingError.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct PidsReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl PidsReader {
  /// Read the volatile hierarchical task count, which can exceed pids.max.
  fn current(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.pids().current()).map(|value| value.value().value)
  }

  /// Read the configured task limit as MaxOr[int]; zero is a valid limit.
  fn max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.pids().max()).map(|value| MaxOr::from_count(value.value()))
  }

  /// Read hierarchical limit events; older kernels or pids_localevents report local events.
  fn events(&self, py: Python<'_>) -> PyResult<PidsEvents> {
    PythonError::read(py, &self.owner, |owner| owner.pids().events()).map(Into::into)
  }
}

/// Immutable, volatile process-limit event counts from pids.events.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PidsEvents {
  /// Number of occurrences of the process limit being hit.
  pub(super) max: u64,
}

impl From<CorePidsEvents> for PidsEvents {
  fn from(value: CorePidsEvents) -> Self { Self { max: value.max().value } }
}
