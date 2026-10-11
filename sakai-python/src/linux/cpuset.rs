//! Python snapshots and readers for CPU and NUMA memory-node placement.

use std::sync::Arc;

use pyo3::{prelude::*, types::PyTuple};
use sakai::v2::cpuset::{CpuList as CoreCpuList, CpusetPartition as CorePartition, MemoryNodeList as CoreNodeList};

use super::PythonError;

/// Fresh placement reads for one pinned cgroup; calls are separate snapshots.
///
/// Missing controllers, root exemptions, and older-kernel omissions raise
/// InterfaceMissingError with the interface path. Malformed contents raise
/// CgroupParseError; unsupported operations raise NotSupportedError and other
/// OS failures raise OSError. Reads never change placement or migrate memory.
/// See https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct CpusetReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl CpusetReader {
  /// Read requested CPU IDs; an empty list means inheritance from ancestors.
  /// Configuration is unchanged by hotplug and can differ from the effective grant.
  fn cpus(&self, py: Python<'_>) -> PyResult<CpuList> {
    PythonError::read(py, &self.owner, |owner| owner.cpuset().cpus()).map(Into::into)
  }

  /// Read online CPU IDs granted to this cgroup; hotplug and ancestor changes
  /// affect the grant. An empty list means no CPUs granted, not inheritance.
  fn cpus_effective(&self, py: Python<'_>) -> PyResult<CpuList> {
    PythonError::read(py, &self.owner, |owner| owner.cpuset().cpus_effective()).map(Into::into)
  }

  /// Read requested NUMA memory-node IDs; an empty list means inheritance.
  /// Configuration is unchanged by hotplug; reading never migrates memory.
  fn mems(&self, py: Python<'_>) -> PyResult<MemoryNodeList> {
    PythonError::read(py, &self.owner, |owner| owner.cpuset().mems()).map(Into::into)
  }

  /// Read online memory-node IDs granted to this cgroup; hotplug and ancestor
  /// changes affect the grant. An empty list means no nodes granted.
  fn mems_effective(&self, py: Python<'_>) -> PyResult<MemoryNodeList> {
    PythonError::read(py, &self.owner, |owner| owner.cpuset().mems_effective()).map(Into::into)
  }

  /// Read partition membership, isolation, and invalid-state diagnostics.
  /// Validity can change with hotplug or ancestor configuration. Older kernels
  /// omit reasons; isolated states are available since Linux 6.1.
  fn partition(&self, py: Python<'_>) -> PyResult<CpusetPartition> {
    PythonError::read(py, &self.owner, |owner| owner.cpuset().partition()).map(Into::into)
  }
}

/// Immutable CPU ID snapshot stored as compact inclusive ranges.
///
/// IDs are unsigned 32-bit identifiers, not CPU counts or per-thread affinity.
/// Configured emptiness means inheritance; effective emptiness means no grant.
/// Effective grants can differ from requests and change with hotplug. Snapshot
/// access performs no kernel I/O. See https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct CpuList {
  inner: CoreCpuList,
}

impl From<CoreCpuList> for CpuList {
  fn from(inner: CoreCpuList) -> Self { Self { inner } }
}

#[pymethods]
impl CpuList {
  /// Stored ascending inclusive (start, end) pairs in an immutable tuple.
  /// Single IDs have equal endpoints; large ranges are never expanded.
  #[getter]
  fn ranges(&self, py: Python<'_>) -> PyResult<Py<PyTuple>> {
    PyTuple::new(py, self.inner.ranges().iter().map(|range| (*range.start(), *range.end()))).map(Bound::unbind)
  }

  /// Test membership in the stored snapshot without kernel I/O.
  /// IDs outside 0..=4294967295 raise OverflowError.
  fn contains(&self, identifier: u32) -> bool { self.inner.contains(identifier) }
}

/// Immutable NUMA memory-node ID snapshot stored as compact inclusive ranges.
///
/// IDs are unsigned 32-bit identifiers, not bytes or node counts. Configured
/// emptiness means inheritance; effective emptiness means no grant. Effective
/// grants can differ from requests and change with hotplug. This is placement
/// policy, not current page residency. Snapshot access performs no kernel I/O.
/// See https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MemoryNodeList {
  inner: CoreNodeList,
}

impl From<CoreNodeList> for MemoryNodeList {
  fn from(inner: CoreNodeList) -> Self { Self { inner } }
}

#[pymethods]
impl MemoryNodeList {
  /// Stored ascending inclusive (start, end) pairs in an immutable tuple.
  /// Single IDs have equal endpoints; large ranges are never expanded.
  #[getter]
  fn ranges(&self, py: Python<'_>) -> PyResult<Py<PyTuple>> {
    PyTuple::new(py, self.inner.ranges().iter().map(|range| (*range.start(), *range.end()))).map(Bound::unbind)
  }

  /// Test membership in the stored snapshot without kernel I/O.
  /// IDs outside 0..=4294967295 raise OverflowError.
  fn contains(&self, identifier: u32) -> bool { self.inner.contains(identifier) }
}

/// Immutable partition-state snapshot; invalid roots behave like members.
///
/// Hotplug and ancestor configuration can change state between fresh reads.
/// Snapshot access performs no kernel I/O. Reasons retain diagnostic kernel
/// text without treating it as a fixed vocabulary. Isolated states and reasons
/// are available since Linux 6.1; older kernels can omit reasons.
/// See https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files.
#[pyclass(frozen, get_all, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct CpusetPartition {
  /// Stored base state: member, root, or isolated; inspect is_invalid too.
  state: &'static str,
  /// Whether the snapshot describes an invalid root; false for members.
  is_invalid: bool,
  /// Kernel reason without parentheses, or None when no reason was reported.
  /// None does not imply a valid partition.
  reason: Option<String>,
}

impl From<CorePartition> for CpusetPartition {
  fn from(value: CorePartition) -> Self {
    Self { state: value.state(), is_invalid: value.is_invalid(), reason: value.reason().map(str::to_owned) }
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use pyo3::exceptions::{PyAttributeError, PyOverflowError, PyTypeError};

  use super::*;

  #[test]
  fn converts_lists_to_compact_immutable_ranges() {
    Python::initialize();
    Python::attach(|py| {
      for (raw, expected) in
        [("", vec![]), ("0-3,5,7-9", vec![(0, 3), (5, 5), (7, 9)]), ("0-4294967295", vec![(0, u32::MAX)])]
      {
        let cpus = assert_ok!(Py::new(py, CpuList::from(assert_ok!(raw.parse::<CoreCpuList>()))));
        let nodes = assert_ok!(Py::new(py, MemoryNodeList::from(assert_ok!(raw.parse::<CoreNodeList>()))));
        for object in [cpus.into_any(), nodes.into_any()] {
          let object = object.bind(py);
          let ranges = assert_ok!(object.getattr("ranges"));
          assert!(ranges.is_instance_of::<PyTuple>());
          assert_eq!(assert_ok!(ranges.extract::<Vec<(u32, u32)>>()), expected);
          assert!(assert_err!(ranges.set_item(0, (1, 2))).is_instance_of::<PyTypeError>(py));
          assert!(assert_err!(object.setattr("ranges", 0)).is_instance_of::<PyAttributeError>(py));
          for id in [0, 4, 5, u32::MAX] {
            assert_eq!(
              assert_ok!(assert_ok!(object.call_method1("contains", (id,))).extract::<bool>()),
              expected.iter().any(|(start, end)| (*start..=*end).contains(&id))
            );
          }
          assert!(assert_err!(object.call_method1("contains", (-1,))).is_instance_of::<PyOverflowError>(py));
          assert!(
            assert_err!(object.call_method1("contains", (u64::from(u32::MAX) + 1,)))
              .is_instance_of::<PyOverflowError>(py)
          );
        }
      }
    });
  }

  #[test]
  fn preserves_partition_validity_and_optional_diagnostics() {
    Python::initialize();
    Python::attach(|py| {
      for (raw, state, is_invalid, reason) in [
        ("member", "member", false, None),
        ("root", "root", false, None),
        ("isolated", "isolated", false, None),
        ("root invalid", "root", true, None),
        ("isolated invalid", "isolated", true, None),
        ("root invalid (future reason)", "root", true, Some("future reason")),
        ("isolated invalid (hotplug)", "isolated", true, Some("hotplug")),
      ] {
        let partition = CpusetPartition::from(assert_ok!(raw.parse::<CorePartition>()));
        assert_eq!(partition, CpusetPartition { state, is_invalid, reason: reason.map(str::to_owned) });
        let object = assert_ok!(Py::new(py, partition));
        assert_eq!(assert_ok!(assert_ok!(object.bind(py).getattr("state")).extract::<String>()), state);
        assert_eq!(assert_ok!(assert_ok!(object.bind(py).getattr("is_invalid")).extract::<bool>()), is_invalid);
        assert_eq!(
          assert_ok!(assert_ok!(object.bind(py).getattr("reason")).extract::<Option<String>>()),
          reason.map(str::to_owned)
        );
        for name in ["state", "is_invalid", "reason"] {
          assert!(assert_err!(object.bind(py).setattr(name, 0)).is_instance_of::<PyAttributeError>(py));
        }
      }
    });
  }
}
