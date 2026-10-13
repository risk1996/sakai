//! Python snapshots and readers for cgroup v2 RDMA resource control.

use std::{collections::BTreeMap, sync::Arc};

use pyo3::{prelude::*, types::PyMappingProxy};
use sakai::v2::rdma::{
  RdmaCurrent as CoreRdmaCurrent, RdmaDevice as CoreRdmaDevice, RdmaDeviceCurrent as CoreRdmaDeviceCurrent,
  RdmaDeviceEvents as CoreRdmaDeviceEvents, RdmaDeviceMax as CoreRdmaDeviceMax, RdmaEvents as CoreRdmaEvents,
  RdmaMax as CoreRdmaMax, RdmaResourceEvents as CoreRdmaResourceEvents,
};

use super::{MaxOr, PythonError, mapping::mapping};

type DeviceKey = (String, Option<u32>);

/// Fresh read-only RDMA reads for one pinned cgroup, keyed by name and optional index.
///
/// Every call opens a fresh interface descriptor; separate snapshots are not
/// atomic together. Usage includes descendants and follows the cgroup that
/// originally owned the resource, even after a task migrates. All resources
/// are integer counts of HCA handles or objects, never byte amounts.
/// Mapping keys are (name, index) tuples. None means the index was omitted,
/// distinct from zero. Linux 7.3 release candidates add indices for duplicate
/// names across network namespaces; older kernels and unique names omit them.
/// See https://github.com/torvalds/linux/blob/v7.3-rc1/kernel/cgroup/rdma.c.
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
/// Keys are (name, index) tuples identifying RDMA adapters. None means the
/// index was omitted, distinct from zero. Linux 7.3 release candidates emit an
/// index for names shared across network namespaces; older kernels and unique
/// names omit it. Name collisions, renames and index reuse can change keys
/// between snapshots. See https://github.com/torvalds/linux/blob/v7.3-rc1/kernel/cgroup/rdma.c.
/// An empty devices map means no devices were reported. Current usage is volatile; peaks record independent high
/// water marks since cgroup creation, available since Linux 7.2. Device
/// removal removes its history. Properties inspect this stored snapshot and
/// perform no I/O. Unknown future resource keys are ignored.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct RdmaCurrent {
  devices: BTreeMap<DeviceKey, RdmaDeviceCurrent>,
}

impl From<CoreRdmaCurrent> for RdmaCurrent {
  fn from(value: CoreRdmaCurrent) -> Self { Self { devices: devices(value.devices()) } }
}

#[pymethods]
impl RdmaCurrent {
  /// Read-only mapping of (name, optional index) tuples to immutable resource counts.
  ///
  /// None means the kernel omitted the index, not zero. This property
  /// returns stored data without opening an interface file.
  #[getter]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.devices, |value| Py::new(py, value.clone()))
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
  devices: BTreeMap<DeviceKey, RdmaDeviceMax>,
}

impl From<CoreRdmaMax> for RdmaMax {
  fn from(value: CoreRdmaMax) -> Self { Self { devices: devices(value.devices()) } }
}

#[pymethods]
impl RdmaMax {
  /// Read-only mapping of (name, optional index) tuples to immutable resource limits.
  ///
  /// None means the kernel omitted the index, not zero. Performs no I/O.
  #[getter]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.devices, |value| Py::new(py, value.clone()))
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
  devices: BTreeMap<DeviceKey, RdmaDeviceEvents>,
}

impl From<CoreRdmaEvents> for RdmaEvents {
  fn from(value: CoreRdmaEvents) -> Self { Self { devices: devices(value.devices()) } }
}

#[pymethods]
impl RdmaEvents {
  /// Read-only mapping of (name, optional index) tuples to immutable per-resource events.
  ///
  /// None means the kernel omitted the index, not zero. Performs no I/O.
  #[getter]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.devices, |value| Py::new(py, value.clone()))
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

fn devices<T: Copy, O: From<T>>(values: &BTreeMap<CoreRdmaDevice, T>) -> BTreeMap<DeviceKey, O> {
  values.iter().map(|(device, value)| ((device.name().to_owned(), device.index()), (*value).into())).collect()
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
        devices: BTreeMap::from([(("mlx4_0".into(), None), RdmaDeviceCurrent {
          hca_handle: 1,
          hca_object: i32::MAX as u64
        }),])
      });
      let current = assert_ok!(Py::new(py, current));
      let devices = assert_ok!(current.bind(py).getattr("devices"));
      let device = assert_ok!(devices.get_item(("mlx4_0", None::<u32>)));
      assert_eq!(assert_ok!(assert_ok!(device.getattr("hca_handle")).extract::<u64>()), 1);
      assert_eq!(assert_ok!(assert_ok!(device.getattr("hca_object")).extract::<u64>()), i32::MAX as u64);
      assert!(assert_err!(device.setattr("hca_handle", 2)).is_instance_of::<PyAttributeError>(py));
      assert!(assert_err!(devices.set_item(("mlx4_0", None::<u32>), 2)).is_instance_of::<PyTypeError>(py));

      let limits = RdmaMax::from(assert_ok!("mlx4_0 hca_handle=0 hca_object=max".parse::<CoreRdmaMax>()));
      let limits = assert_ok!(Py::new(py, limits));
      let devices = assert_ok!(limits.bind(py).getattr("devices"));
      let device = assert_ok!(devices.get_item(("mlx4_0", None::<u32>)));
      let handle = assert_ok!(device.getattr("hca_handle"));
      assert_eq!(assert_ok!(assert_ok!(handle.getattr("value")).extract::<u64>()), 0);
      let object = assert_ok!(device.getattr("hca_object"));
      assert!(assert_ok!(assert_ok!(object.getattr("is_max")).extract::<bool>()));
      assert!(assert_err!(object.getattr("value")).is_instance_of::<PyValueError>(py));
      assert!(assert_err!(devices.set_item(("mlx4_0", None::<u32>), 2)).is_instance_of::<PyTypeError>(py));
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
        devices: BTreeMap::from([(("mlx4_0".into(), None), RdmaDeviceEvents {
          hca_handle: RdmaResourceEvents { max: 5, alloc_fail: 7 },
          hca_object: RdmaResourceEvents { max: 0, alloc_fail: 1 },
        }),])
      });
      let events = assert_ok!(Py::new(py, events));
      let devices = assert_ok!(events.bind(py).getattr("devices"));
      let device = assert_ok!(devices.get_item(("mlx4_0", None::<u32>)));
      let handles = assert_ok!(device.getattr("hca_handle"));
      assert_eq!(assert_ok!(assert_ok!(handles.getattr("alloc_fail")).extract::<u64>()), 7);
      assert!(assert_err!(handles.setattr("max", 0)).is_instance_of::<PyAttributeError>(py));
      assert!(assert_err!(device.setattr("hca_handle", 0)).is_instance_of::<PyAttributeError>(py));
      assert!(assert_err!(devices.set_item(("mlx4_0", None::<u32>), 0)).is_instance_of::<PyTypeError>(py));
    });
  }

  #[test]
  fn preserves_indexed_and_name_only_keys_in_all_python_snapshots() {
    Python::initialize();
    Python::attach(|py| {
      let keys = [("rxe0".to_owned(), Some(0)), ("rxe0".to_owned(), Some(5)), ("mlx4_0".to_owned(), None)];
      for (raw, expected) in [
        (include_str!("../../../sakai/src/cgroup/v2/fixtures/p6_rdma_index/rdma.current"), [[1_u64, 20], [2, 30], [
          3, 40,
        ]]),
        (include_str!("../../../sakai/src/cgroup/v2/fixtures/p6_rdma_index/rdma.peak"), [[10, 200], [20, 300], [
          30, 400,
        ]]),
      ] {
        let snapshot = RdmaCurrent::from(assert_ok!(raw.parse::<CoreRdmaCurrent>()));
        assert_eq!(snapshot, RdmaCurrent {
          devices: keys
            .iter()
            .cloned()
            .zip(expected.map(|[hca_handle, hca_object]| RdmaDeviceCurrent { hca_handle, hca_object }))
            .collect()
        });
        let object = assert_ok!(Py::new(py, snapshot));
        let devices = assert_ok!(object.bind(py).getattr("devices"));
        assert_eq!(assert_ok!(devices.len()), 3);
        for (key, expected) in keys.iter().zip(expected) {
          let device = assert_ok!(devices.get_item(key));
          let actual =
            ["hca_handle", "hca_object"].map(|field| assert_ok!(assert_ok!(device.getattr(field)).extract::<u64>()));
          assert_eq!(actual, expected);
        }
        assert!(assert_err!(devices.set_item(("rxe0", Some(0)), 0)).is_instance_of::<PyTypeError>(py));
        assert!(assert_err!(object.bind(py).setattr("devices", 0)).is_instance_of::<PyAttributeError>(py));
      }
      let limits = RdmaMax::from(assert_ok!(
        include_str!("../../../sakai/src/cgroup/v2/fixtures/p6_rdma_index/rdma.max").parse::<CoreRdmaMax>()
      ));
      let object = assert_ok!(Py::new(py, limits));
      let devices = assert_ok!(object.bind(py).getattr("devices"));
      assert_eq!(assert_ok!(devices.len()), 3);
      for (key, expected) in keys.iter().zip([[Some(0_u64), Some(20)], [Some(2), None], [None, Some(40)]]) {
        let device = assert_ok!(devices.get_item(key));
        for (field, expected) in ["hca_handle", "hca_object"].into_iter().zip(expected) {
          let limit = assert_ok!(device.getattr(field));
          assert_eq!(assert_ok!(assert_ok!(limit.getattr("is_max")).extract::<bool>()), expected.is_none());
          match expected {
            | Some(value) => assert_eq!(assert_ok!(assert_ok!(limit.getattr("value")).extract::<u64>()), value),
            | None => assert!(assert_err!(limit.getattr("value")).is_instance_of::<PyValueError>(py)),
          }
        }
      }
      assert!(assert_err!(devices.set_item(("rxe0", Some(5)), 0)).is_instance_of::<PyTypeError>(py));
      assert!(assert_err!(object.bind(py).setattr("devices", 0)).is_instance_of::<PyAttributeError>(py));
      for (raw, expected) in [
        (include_str!("../../../sakai/src/cgroup/v2/fixtures/p6_rdma_index/rdma.events"), [
          [1_u64, 2, 3, 4],
          [5, 6, 7, 8],
          [9, 10, 11, 12],
        ]),
        (include_str!("../../../sakai/src/cgroup/v2/fixtures/p6_rdma_index/rdma.events.local"), [
          [4, 5, 6, 7],
          [0, 1, 2, 3],
          [8, 9, 10, 11],
        ]),
      ] {
        let snapshot = RdmaEvents::from(assert_ok!(raw.parse::<CoreRdmaEvents>()));
        assert_eq!(snapshot, RdmaEvents {
          devices: keys
            .iter()
            .cloned()
            .zip(expected.map(|[handle_max, handle_fail, object_max, object_fail]| RdmaDeviceEvents {
              hca_handle: RdmaResourceEvents { max: handle_max, alloc_fail: handle_fail },
              hca_object: RdmaResourceEvents { max: object_max, alloc_fail: object_fail },
            }))
            .collect()
        });
        let object = assert_ok!(Py::new(py, snapshot));
        let devices = assert_ok!(object.bind(py).getattr("devices"));
        assert_eq!(assert_ok!(devices.len()), 3);
        for (key, [handle_max, handle_fail, object_max, object_fail]) in keys.iter().zip(expected) {
          let device = assert_ok!(devices.get_item(key));
          for (resource, expected) in
            [("hca_handle", [handle_max, handle_fail]), ("hca_object", [object_max, object_fail])]
          {
            let events = assert_ok!(device.getattr(resource));
            let actual =
              ["max", "alloc_fail"].map(|field| assert_ok!(assert_ok!(events.getattr(field)).extract::<u64>()));
            assert_eq!(actual, expected);
          }
        }
        assert!(assert_err!(devices.set_item(("rxe0", Some(0)), 0)).is_instance_of::<PyTypeError>(py));
        assert!(assert_err!(object.bind(py).setattr("devices", 0)).is_instance_of::<PyAttributeError>(py));
      }
    });
  }
}
