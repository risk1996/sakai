//! Python readers and immutable device-memory region snapshots.

use std::{collections::BTreeMap, sync::Arc};

#[cfg(test)]
use pyo3::types::PyDict;
use pyo3::{
  prelude::*,
  types::{IntoPyDict, PyMappingProxy},
};
use sakai::v2::dmem::{
  DmemCapacity as CoreDmemCapacity, DmemCurrent as CoreDmemCurrent, DmemLimits as CoreDmemLimits,
  DmemPeak as CoreDmemPeak,
};
use uom::si::information::byte;

use super::{MaxOr, PythonError, mapping::mapping};

/// Fresh device-memory reads for one pinned cgroup, available since Linux 6.14.
///
/// Values are bytes regardless of device or system page size. Region names are
/// opaque driver identifiers such as "drm/0000:03:00.0/vram0", not paths to open.
/// Each call performs read-only I/O and returns an immutable snapshot; separate
/// calls are not atomic. Requires CONFIG_CGROUP_DMEM and controller availability.
///
/// Missing interfaces, including older kernels and root exemptions, raise
/// InterfaceMissingError. Unsupported reads raise NotSupportedError;
/// malformed contents raise CgroupParseError; other I/O failures raise OSError
/// with errno and filename. Exceptions retain the diagnostic interface path.
/// See https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct DmemReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl DmemReader {
  /// Read registered device-memory region capacities in bytes.
  ///
  /// The interface exists only at the hierarchy root; non-root reads raise
  /// InterfaceMissingError. Capacity includes kernel-reserved device memory
  /// and is not a guarantee of allocatable cgroup memory. An empty map means
  /// no regions were reported. Since Linux 6.14; see DmemReader for read errors
  /// and the Linux interface reference.
  fn capacity(&self, py: Python<'_>) -> PyResult<DmemCapacity> {
    PythonError::read(py, &self.owner, |owner| owner.dmem().capacity()).map(Into::into)
  }

  /// Read current hierarchical device-memory usage as MaxOr[int] bytes per region.
  ///
  /// Counts include this cgroup and its descendants and can change immediately
  /// after a read. Zero is measured zero; an absent key means the region was
  /// not reported. Kernel implementations also expose this file at the root,
  /// although the interface guide describes it as non-root. Since Linux 6.14;
  /// see DmemReader for read errors and the Linux interface reference.
  ///
  /// max preserves the kernel's PAGE_COUNTER_MAX sentinel; it is not an
  /// unlimited configured limit. The sentinel depends on kernel architecture
  /// and system page size. See https://github.com/torvalds/linux/blob/v6.14/kernel/cgroup/dmem.c.
  fn current(&self, py: Python<'_>) -> PyResult<DmemCurrent> {
    PythonError::read(py, &self.owner, |owner| owner.dmem().current()).map(Into::into)
  }

  /// Read historical hierarchical device-memory peaks as MaxOr[int] bytes.
  ///
  /// Since Linux 7.3 release candidates; non-root only. Each region records an
  /// independent lifetime peak, including transient charges later canceled by
  /// ancestor-limit failures. max preserves the PAGE_COUNTER_MAX sentinel;
  /// reading never resets peaks. Region removal removes its history. See
  /// https://github.com/torvalds/linux/blob/v7.3-rc1/kernel/cgroup/dmem.c and
  /// DmemReader for read errors.
  fn peak(&self, py: Python<'_>) -> PyResult<DmemPeak> {
    PythonError::read(py, &self.owner, |owner| owner.dmem().peak()).map(Into::into)
  }

  /// Read configured hard eviction protections as MaxOr[int] byte amounts.
  ///
  /// Zero disables protection; max requests protection without a numeric
  /// upper boundary. Effective protection depends on ancestors and sibling
  /// demand. The file is absent at the root. Since Linux 6.14; see DmemReader
  /// for read errors and the Linux interface reference.
  fn min(&self, py: Python<'_>) -> PyResult<DmemLimits> {
    PythonError::read(py, &self.owner, |owner| owner.dmem().min()).map(Into::into)
  }

  /// Read configured best-effort eviction protections as MaxOr[int] bytes.
  ///
  /// Zero disables protection; max requests protection without a numeric
  /// upper boundary. Effective protection depends on ancestors and sibling
  /// demand. The file is absent at the root. Since Linux 6.14; see DmemReader
  /// for read errors and the Linux interface reference.
  fn low(&self, py: Python<'_>) -> PyResult<DmemLimits> {
    PythonError::read(py, &self.owner, |owner| owner.dmem().low()).map(Into::into)
  }

  /// Read configured hierarchical limits as MaxOr[int] byte amounts.
  ///
  /// Zero is valid. max removes this cgroup's limit; ancestor limits still
  /// apply. The file is absent at the root. Since Linux 6.14; see DmemReader
  /// for read errors and the Linux interface reference.
  fn max(&self, py: Python<'_>) -> PyResult<DmemLimits> {
    PythonError::read(py, &self.owner, |owner| owner.dmem().max()).map(Into::into)
  }
}

/// Immutable snapshot of root-only dmem.capacity values in bytes.
///
/// Capacity includes kernel-reserved memory and does not guarantee allocatable
/// memory. Empty means no regions were reported; arbitrary driver names are
/// retained. Region membership may change between reads. Properties inspect
/// stored values without I/O. Since Linux 6.14; see DmemReader for the reference.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct DmemCapacity {
  regions: BTreeMap<String, u64>,
}

impl From<CoreDmemCapacity> for DmemCapacity {
  fn from(value: CoreDmemCapacity) -> Self {
    Self { regions: value.regions().iter().map(|(name, value)| (name.clone(), value.get::<byte>())).collect() }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl DmemCapacity {
  /// Read-only mapping from opaque region names to integer byte capacities.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[str, int]", imports = ("collections.abc",)))]
  fn regions(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, (&self.regions).into_py_dict(py)?.as_mapping()).unbind())
  }
}

/// Immutable, volatile hierarchical dmem.current usage in bytes.
///
/// Values include this cgroup and its descendants. Zero differs from an absent
/// region; an empty map is valid. Usage and region membership may change between
/// reads. Properties inspect stored values without I/O. Since Linux 6.14; see
/// DmemReader for the Linux interface reference.
/// max preserves the kernel's PAGE_COUNTER_MAX sentinel, not an unlimited
/// configured limit; its numeric boundary depends on kernel architecture and
/// system page size. Finite quantities are byte amounts wrapped in MaxOr[int].
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct DmemCurrent {
  regions: BTreeMap<String, MaxOr>,
}

impl From<CoreDmemCurrent> for DmemCurrent {
  fn from(value: CoreDmemCurrent) -> Self {
    Self { regions: value.regions().iter().map(|(name, value)| (name.clone(), MaxOr::from_bytes(*value))).collect() }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl DmemCurrent {
  /// Read-only mapping from opaque region names to MaxOr[int] hierarchical bytes.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[str, MaxOr[int]]", imports = ("collections.abc",)))]
  fn regions(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.regions, |value| Py::new(py, *value))
  }
}

/// Immutable hierarchical device-memory peak snapshot, since Linux 7.3 release candidates.
///
/// Each region maps to MaxOr[int] bytes. max preserves the kernel's
/// PAGE_COUNTER_MAX sentinel, not an unlimited configured limit. Region peaks
/// are independent and lifetime-scoped; device removal removes their history.
/// Properties inspect stored snapshots without I/O; reads never reset peaks.
/// See https://github.com/torvalds/linux/blob/v7.3-rc1/kernel/cgroup/dmem.c.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct DmemPeak {
  regions: BTreeMap<String, MaxOr>,
}

impl From<CoreDmemPeak> for DmemPeak {
  fn from(value: CoreDmemPeak) -> Self {
    Self { regions: value.regions().iter().map(|(name, value)| (name.clone(), MaxOr::from_bytes(*value))).collect() }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl DmemPeak {
  /// Read-only mapping from opaque region names to MaxOr[int] historical byte peaks.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[str, MaxOr[int]]", imports = ("collections.abc",)))]
  fn regions(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.regions, |value| Py::new(py, *value))
  }
}

/// Immutable configuration from dmem.max, dmem.min, or dmem.low.
///
/// Each region maps to MaxOr[int], with finite amounts measured in bytes.
/// max means unlimited here for a usage limit or protection without a numeric
/// boundary for min/low. These are configured values; ancestor limits and
/// protection, and sibling demand, can affect effective behavior. Zero is valid.
/// Empty means no regions were reported. Configuration and region membership
/// can change between reads; properties inspect stored values without I/O.
/// Since Linux 6.14; see DmemReader for the Linux interface reference.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct DmemLimits {
  regions: BTreeMap<String, MaxOr>,
}

impl From<CoreDmemLimits> for DmemLimits {
  fn from(value: CoreDmemLimits) -> Self {
    Self { regions: value.regions().iter().map(|(name, value)| (name.clone(), MaxOr::from_bytes(*value))).collect() }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl DmemLimits {
  /// Read-only mapping from opaque region names to immutable MaxOr[int] bytes.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[str, MaxOr[int]]", imports = ("collections.abc",)))]
  fn regions(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.regions, |value| Py::new(py, *value))
  }
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;

  use super::{
    super::tests::{check_frozen, check_limit, readonly},
    *,
  };

  #[test]
  fn converts_byte_snapshots_into_immutable_region_maps() {
    Python::initialize();
    Python::attach(|py| {
      let input = include_str!("../../../sakai/src/cgroup/v2/fixtures/p6_dmem/dmem.current");
      let expected = BTreeMap::from([
        ("drm/0000:03:00.0/vram0".to_owned(), 12_550_144_u64),
        ("drm/0000:03:00.0/stolen".to_owned(), 0),
        ("future/device".to_owned(), u64::MAX),
      ]);
      let capacity = assert_ok!(Py::new(py, DmemCapacity::from(assert_ok!(input.parse::<CoreDmemCapacity>()))));
      let mapping = readonly(capacity.bind(py).as_any(), "regions", "future/device");
      let copied = assert_ok!(py.get_type::<PyDict>().call1((&mapping,)));
      assert_eq!(assert_ok!(copied.extract::<BTreeMap<String, u64>>()), expected);
      assert!(DmemCurrent::from(assert_ok!("".parse::<CoreDmemCurrent>())).regions.is_empty());
      assert!(DmemCapacity::from(assert_ok!("".parse::<CoreDmemCapacity>())).regions.is_empty());
    });
  }

  #[test]
  fn converts_finite_and_max_protections_without_losing_unsigned_values() {
    Python::initialize();
    Python::attach(|py| {
      for input in ["device 18446744073709551615\nother max\nzero 0", ""] {
        let limits = assert_ok!(Py::new(py, DmemLimits::from(assert_ok!(input.parse::<CoreDmemLimits>()))));
        let current = assert_ok!(Py::new(py, DmemCurrent::from(assert_ok!(input.parse::<CoreDmemCurrent>()))));
        let peak = assert_ok!(Py::new(py, DmemPeak::from(assert_ok!(input.parse::<CoreDmemPeak>()))));
        for object in [limits.into_any(), current.into_any(), peak.into_any()] {
          let mapping = readonly(object.bind(py), "regions", "device");
          if input.is_empty() {
            assert_eq!(assert_ok!(mapping.len()), 0);
          } else {
            for (name, expected) in [("device", Some(u64::MAX)), ("other", None), ("zero", Some(0))] {
              let limit = assert_ok!(mapping.get_item(name));
              check_limit(&limit, expected);
              check_frozen(&limit, &["value"]);
            }
          }
        }
      }
    });
  }
}
