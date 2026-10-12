//! Pids bindings and conversions.

use std::sync::Arc;

use pyo3::prelude::*;
use sakai::v2::pids::PidsEvents as CorePidsEvents;

use super::{MaxOr, PythonError};

/// Fresh process-number reads for one pinned cgroup. Counts include threads.
///
/// Separate calls are not atomic together. Missing files raise InterfaceMissingError.
/// Malformed contents raise CgroupParseError; unsupported reads raise
/// NotSupportedError; other operating-system failures raise OSError with the
/// interface path and original cause. The hierarchy root lacks these files.
/// See https://docs.kernel.org/admin-guide/cgroup-v2.html#pid.
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

  /// Read the historical hierarchical task-count peak, including threads.
  ///
  /// Since Linux 6.1. Records lifetime usage, including transient charges later
  /// canceled by an ancestor-limit failure. Reading never resets the peak.
  /// See https://github.com/torvalds/linux/blob/v6.1/kernel/cgroup/pids.c and
  /// PidsReader for read errors.
  fn peak(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.pids().peak()).map(|value| value.value().value)
  }

  /// Read the configured task limit as MaxOr[int]; zero is a valid limit.
  fn max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.pids().max()).map(|value| MaxOr::from_count(value.value()))
  }

  /// Read hierarchical limit events; older kernels or pids_localevents report local events.
  fn events(&self, py: Python<'_>) -> PyResult<PidsEvents> {
    PythonError::read(py, &self.owner, |owner| owner.pids().events()).map(Into::into)
  }

  /// Read local process-limit events, available since Linux 6.11.
  ///
  /// Normally counts failures at this cgroup's own limit, including descendant
  /// requests. With pids_localevents, counts fork failures originating here,
  /// including ancestor-limit hits. See PidsReader for errors and
  /// https://github.com/torvalds/linux/blob/v6.11/kernel/cgroup/pids.c.
  fn events_local(&self, py: Python<'_>) -> PyResult<PidsEvents> {
    PythonError::read(py, &self.owner, |owner| owner.pids().events_local()).map(Into::into)
  }
}

/// Immutable, volatile process-limit event counts from pids.events or pids.events.local.
///
/// Hierarchical versus local semantics depend on the reader, kernel version,
/// and pids_localevents mount option. Properties inspect stored counts without
/// I/O. See PidsReader and https://docs.kernel.org/admin-guide/cgroup-v2.html#pid.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PidsEvents {
  /// Number of occurrences of the process limit being hit.
  pub(super) max: u64,
}

impl From<CorePidsEvents> for PidsEvents {
  fn from(value: CorePidsEvents) -> Self { Self { max: value.max().value } }
}
