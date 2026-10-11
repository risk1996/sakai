//! Python snapshots and readers for cgroup v2 RDMA resource control.

use std::{collections::BTreeMap, sync::Arc};

use pyo3::{
  prelude::*,
  types::{PyAny, PyDict, PyMappingProxy},
};
use sakai::v2::rdma::{
  RdmaCurrent as CoreRdmaCurrent, RdmaDeviceCurrent as CoreRdmaDeviceCurrent, RdmaDeviceEvents as CoreRdmaDeviceEvents,
  RdmaDeviceMax as CoreRdmaDeviceMax, RdmaEvents as CoreRdmaEvents, RdmaMax as CoreRdmaMax,
  RdmaResourceEvents as CoreRdmaResourceEvents,
};

use super::{MaxOr, PythonError};

/// Fresh read-only RDMA reads for one pinned cgroup, keyed by adapter name.
///
/// Every call opens a fresh interface descriptor; separate snapshots are not
/// atomic together. Usage includes descendants and follows the cgroup that
/// originally owned the resource, even after a task migrates. All resources
/// are integer counts of HCA handles or objects, never byte amounts.
///
/// Missing files, including root exemptions, disabled controllers, kernel
/// configuration omissions, and older kernels, raise InterfaceMissingError.
/// Malformed contents raise CgroupParseError. Unsupported operations raise
/// NotSupportedError; other operating-system failures raise OSError.
/// Reads never change resource limits, reset peaks, or acknowledge events.
///
/// See https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct RdmaReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl RdmaReader {
  /// Read a fresh snapshot of hierarchical HCA handle and object counts.
  ///
  /// Device usage changes as resources are allocated and freed. An empty map
  /// means no devices were reported. Missing rdma.current raises
  /// InterfaceMissingError; invalid contents raise CgroupParseError; other read
  /// failures retain the interface path and operating-system cause.
  fn current(&self, py: Python<'_>) -> PyResult<RdmaCurrent> {
    PythonError::read(py, &self.owner, |owner| owner.rdma().current()).map(Into::into)
  }

  /// Read configured per-device HCA limits as MaxOr[int] resource counts.
  ///
  /// Zero is a valid limit. max means no configured limit here; ancestor
  /// limits still apply. This snapshot can change between reads. Missing
  /// rdma.max raises InterfaceMissingError; invalid contents raise CgroupParseError;
  /// other failures retain the interface path and operating-system cause.
  fn max(&self, py: Python<'_>) -> PyResult<RdmaMax> {
    PythonError::read(py, &self.owner, |owner| owner.rdma().max()).map(Into::into)
  }

  /// Read per-device hierarchical resource high-water marks, in counts.
  ///
  /// Since Linux 7.2. Each peak is independent and records usage since cgroup
  /// creation; device removal removes its history. This read never resets
  /// peaks. Missing rdma.peak on older kernels raises InterfaceMissingError;
  /// invalid contents raise CgroupParseError; other failures preserve their cause.
  /// See https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c.
  fn peak(&self, py: Python<'_>) -> PyResult<RdmaCurrent> {
    PythonError::read(py, &self.owner, |owner| owner.rdma().peak()).map(Into::into)
  }

  /// Read hierarchical per-resource allocation-failure event counts.
  ///
  /// Since Linux 7.2. Includes descendants' max and alloc_fail events. Counters
  /// can increase between reads; device removal removes its history. Missing
  /// rdma.events raises InterfaceMissingError; invalid contents raise
  /// CgroupParseError; other failures preserve the operating-system cause.
  /// See https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c.
  fn events(&self, py: Python<'_>) -> PyResult<RdmaEvents> {
    PythonError::read(py, &self.owner, |owner| owner.rdma().events()).map(Into::into)
  }

  /// Read local per-resource allocation-failure event counts.
  ///
  /// Since Linux 7.2. Local max counts requests rejected by this cgroup's own
  /// limit, including requests from descendants. Local alloc_fail counts
  /// requests originating here, including rejections by ancestor limits.
  /// Missing rdma.events.local raises InterfaceMissingError; invalid contents
  /// raise CgroupParseError; other failures preserve the operating-system cause.
  /// See https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c.
  fn events_local(&self, py: Python<'_>) -> PyResult<RdmaEvents> {
    PythonError::read(py, &self.owner, |owner| owner.rdma().events_local()).map(Into::into)
  }
}

/// Immutable snapshot of hierarchical RDMA current or peak resource counts.
///
/// Device names identify RDMA adapters. An empty devices map means no devices
/// were reported. Current usage is volatile; peaks record independent high
/// water marks since cgroup creation, available since Linux 7.2. Device
/// removal removes its history. Properties inspect this stored snapshot and
/// perform no I/O. Unknown future resource keys are ignored.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct RdmaCurrent {
  devices: BTreeMap<String, RdmaDeviceCurrent>,
}

impl From<CoreRdmaCurrent> for RdmaCurrent {
  fn from(value: CoreRdmaCurrent) -> Self {
    Self { devices: value.devices().iter().map(|(device, value)| (device.clone(), (*value).into())).collect() }
  }
}

#[pymethods]
impl RdmaCurrent {
  /// Read-only mapping of adapter names to immutable resource counts.
  ///
  /// This property returns stored data without opening an interface file.
  #[getter]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.devices, |value| Py::new(py, value.clone()).map(Py::into_any))
  }
}

/// Immutable hierarchical handle and object counts for one RDMA adapter.
///
/// HCA handles are userspace adapter contexts; HCA objects include queue
/// pairs, memory regions, and completion queues. All quantities are counts.
/// The kernel's max token for current or peak usage represents exactly
/// 2,147,483,647 and is exposed as that integer, not an unlimited value.
/// Properties inspect stored values without performing I/O.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RdmaDeviceCurrent {
  /// Stored count of HCA handles, or their independent historical peak.
  hca_handle: u64,
  /// Stored count of HCA objects, or their independent historical peak.
  hca_object: u64,
}

impl From<CoreRdmaDeviceCurrent> for RdmaDeviceCurrent {
  fn from(value: CoreRdmaDeviceCurrent) -> Self {
    Self { hca_handle: value.hca_handle().value, hca_object: value.hca_object().value }
  }
}

/// Immutable per-device RDMA resource limits, measured in counts.
///
/// Configuration can change between reads. max means no configured limit at
/// this cgroup; ancestors may still constrain allocation. An empty map means
/// no devices were reported. Properties return stored data without I/O.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct RdmaMax {
  devices: BTreeMap<String, RdmaDeviceMax>,
}

impl From<CoreRdmaMax> for RdmaMax {
  fn from(value: CoreRdmaMax) -> Self {
    Self { devices: value.devices().iter().map(|(device, value)| (device.clone(), (*value).into())).collect() }
  }
}

#[pymethods]
impl RdmaMax {
  /// Read-only mapping of adapter names to immutable resource limits.
  #[getter]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.devices, |value| Py::new(py, value.clone()).map(Py::into_any))
  }
}

/// Immutable limits for one RDMA adapter, each a MaxOr[int] resource count.
///
/// Zero is a valid limit. max means no limit here; ancestor limits still
/// apply. Reading a property does not refresh this configuration snapshot.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Clone)]
pub(super) struct RdmaDeviceMax {
  /// Stored HCA handle limit in counts, or max.
  hca_handle: MaxOr,
  /// Stored HCA object limit in counts, or max.
  hca_object: MaxOr,
}

impl From<CoreRdmaDeviceMax> for RdmaDeviceMax {
  fn from(value: CoreRdmaDeviceMax) -> Self {
    Self { hca_handle: MaxOr::from_count(value.hca_handle()), hca_object: MaxOr::from_count(value.hca_object()) }
  }
}

/// Immutable hierarchical or local per-device RDMA event counts.
///
/// Since Linux 7.2. Counters may increase between interface reads; device
/// removal removes its history. An empty map means no devices were reported.
/// Unknown future event keys are ignored. Properties inspect stored values
/// and perform no I/O.
/// See https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct RdmaEvents {
  devices: BTreeMap<String, RdmaDeviceEvents>,
}

impl From<CoreRdmaEvents> for RdmaEvents {
  fn from(value: CoreRdmaEvents) -> Self {
    Self { devices: value.devices().iter().map(|(device, value)| (device.clone(), (*value).into())).collect() }
  }
}

#[pymethods]
impl RdmaEvents {
  /// Read-only mapping of adapter names to immutable per-resource events.
  #[getter]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.devices, |value| Py::new(py, value.clone()).map(Py::into_any))
  }
}

/// Immutable allocation-event counters for one RDMA adapter, since Linux 7.2.
///
/// Properties return stored events without refreshing the snapshot.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RdmaDeviceEvents {
  /// Stored allocation-event counts for HCA handles.
  hca_handle: RdmaResourceEvents,
  /// Stored allocation-event counts for HCA objects.
  hca_object: RdmaResourceEvents,
}

impl From<CoreRdmaDeviceEvents> for RdmaDeviceEvents {
  fn from(value: CoreRdmaDeviceEvents) -> Self {
    Self { hca_handle: value.hca_handle().into(), hca_object: value.hca_object().into() }
  }
}

/// Immutable allocation-failure counts for one RDMA resource, since Linux 7.2.
///
/// Whether counters include descendants is determined by the reader used:
/// events() is hierarchical; events_local() is local. Properties inspect the
/// stored snapshot without I/O. Counts may increase on the next fresh read.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RdmaResourceEvents {
  /// Failures against this cgroup's own limit, including descendant requests.
  ///
  /// Hierarchical snapshots also include descendant cgroups' limit events.
  max: u64,
  /// Requests rejected by this cgroup's or an ancestor's limit.
  ///
  /// Local snapshots count requests originating here; hierarchical snapshots
  /// also include requests originating in descendants.
  alloc_fail: u64,
}

impl From<CoreRdmaResourceEvents> for RdmaResourceEvents {
  fn from(value: CoreRdmaResourceEvents) -> Self {
    Self { max: value.max().value, alloc_fail: value.alloc_fail().value }
  }
}

fn mapping<T>(
  py: Python<'_>,
  values: &BTreeMap<String, T>,
  convert: impl Fn(&T) -> PyResult<Py<PyAny>>,
) -> PyResult<Py<PyMappingProxy>> {
  let dict = PyDict::new(py);
  for (device, value) in values {
    dict.set_item(device, convert(value)?)?;
  }
  Ok(PyMappingProxy::new(py, dict.as_mapping()).unbind())
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use pyo3::exceptions::{PyAttributeError, PyTypeError, PyValueError};

  use super::*;

  #[test]
  fn converts_resource_snapshots_and_freezes_devices() {
    Python::initialize();
    Python::attach(|py| {
      let current = RdmaCurrent::from(assert_ok!("mlx4_0 hca_handle=1 hca_object=max".parse::<CoreRdmaCurrent>()));
      assert_eq!(current, RdmaCurrent {
        devices: BTreeMap::from([("mlx4_0".into(), RdmaDeviceCurrent { hca_handle: 1, hca_object: i32::MAX as u64 }),])
      });
      let current = assert_ok!(Py::new(py, current));
      let devices = assert_ok!(current.bind(py).getattr("devices"));
      let device = assert_ok!(devices.get_item("mlx4_0"));
      assert_eq!(assert_ok!(assert_ok!(device.getattr("hca_handle")).extract::<u64>()), 1);
      assert_eq!(assert_ok!(assert_ok!(device.getattr("hca_object")).extract::<u64>()), i32::MAX as u64);
      assert!(assert_err!(device.setattr("hca_handle", 2)).is_instance_of::<PyAttributeError>(py));
      assert!(assert_err!(devices.set_item("mlx4_0", 2)).is_instance_of::<PyTypeError>(py));

      let limits = RdmaMax::from(assert_ok!("mlx4_0 hca_handle=0 hca_object=max".parse::<CoreRdmaMax>()));
      let limits = assert_ok!(Py::new(py, limits));
      let devices = assert_ok!(limits.bind(py).getattr("devices"));
      let device = assert_ok!(devices.get_item("mlx4_0"));
      let handle = assert_ok!(device.getattr("hca_handle"));
      assert_eq!(assert_ok!(assert_ok!(handle.getattr("value")).extract::<u64>()), 0);
      let object = assert_ok!(device.getattr("hca_object"));
      assert!(assert_ok!(assert_ok!(object.getattr("is_max")).extract::<bool>()));
      assert!(assert_err!(object.getattr("value")).is_instance_of::<PyValueError>(py));
      assert!(assert_err!(devices.set_item("mlx4_0", 2)).is_instance_of::<PyTypeError>(py));
      assert!(assert_err!(device.setattr("hca_handle", 2)).is_instance_of::<PyAttributeError>(py));
    });
  }

  #[test]
  fn converts_events_and_freezes_nested_counters() {
    Python::initialize();
    Python::attach(|py| {
      let events = RdmaEvents::from(assert_ok!(
        "mlx4_0 hca_handle.max=5 hca_handle.alloc_fail=7 hca_object.max=0 hca_object.alloc_fail=1"
          .parse::<CoreRdmaEvents>()
      ));
      assert_eq!(events, RdmaEvents {
        devices: BTreeMap::from([("mlx4_0".into(), RdmaDeviceEvents {
          hca_handle: RdmaResourceEvents { max: 5, alloc_fail: 7 },
          hca_object: RdmaResourceEvents { max: 0, alloc_fail: 1 },
        }),])
      });
      let events = assert_ok!(Py::new(py, events));
      let devices = assert_ok!(events.bind(py).getattr("devices"));
      let device = assert_ok!(devices.get_item("mlx4_0"));
      let handles = assert_ok!(device.getattr("hca_handle"));
      assert_eq!(assert_ok!(assert_ok!(handles.getattr("alloc_fail")).extract::<u64>()), 7);
      assert!(assert_err!(handles.setattr("max", 0)).is_instance_of::<PyAttributeError>(py));
      assert!(assert_err!(device.setattr("hca_handle", 0)).is_instance_of::<PyAttributeError>(py));
      assert!(assert_err!(devices.set_item("mlx4_0", 0)).is_instance_of::<PyTypeError>(py));
    });
  }
}
