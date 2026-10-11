//! Python snapshots and readers for the cgroup v2 I/O controller.

use std::{collections::BTreeMap, sync::Arc};

use pyo3::{
  prelude::*,
  types::{IntoPyDict, PyAny, PyDict, PyMappingProxy},
};
use sakai::v2::io::{
  Device, IoDeviceMax as CoreIoDeviceMax, IoDeviceStat as CoreIoDeviceStat, IoLatency as CoreIoLatency,
  IoMax as CoreIoMax, IoStat as CoreIoStat, IoWeight as CoreIoWeight,
};
use uom::si::{frequency::hertz, information::byte, information_rate::byte_per_second, time::nanosecond};

use super::{MaxOr, Pressure, PythonError, PythonLimitValue};

type DeviceKey = (u32, u32);

/// Fresh I/O reads for one pinned cgroup; each call is a separate snapshot.
///
/// Missing files, including optional kernel features and root exemptions,
/// raise InterfaceMissingError with the interface path. Reads never write PSI
/// triggers or change controller configuration.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct IoReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl IoReader {
  /// Read hierarchical per-device byte and operation counters.
  fn stat(&self, py: Python<'_>) -> PyResult<IoStat> {
    PythonError::read(py, &self.owner, |owner| owner.io().stat()).map(Into::into)
  }

  /// Read I/O pressure averages as ratios and stall times in nanoseconds.
  fn pressure(&self, py: Python<'_>) -> PyResult<Pressure> {
    PythonError::read(py, &self.owner, |owner| owner.io().pressure()).map(Into::into)
  }

  /// Read the default I/O scheduling weight and explicit device overrides.
  fn weight(&self, py: Python<'_>) -> PyResult<IoWeight> {
    PythonError::read(py, &self.owner, |owner| owner.io().weight()).map(Into::into)
  }

  /// Read per-device bandwidth and IOPS limits as MaxOr[int] values.
  fn max(&self, py: Python<'_>) -> PyResult<IoMax> {
    PythonError::read(py, &self.owner, |owner| owner.io().max()).map(Into::into)
  }

  /// Read configured per-device latency protection targets in nanoseconds.
  fn latency(&self, py: Python<'_>) -> PyResult<IoLatency> {
    PythonError::read(py, &self.owner, |owner| owner.io().latency()).map(Into::into)
  }
}

/// Immutable, volatile I/O counters keyed by (major, minor) device tuples.
///
/// An empty map means no devices were reported. Missing counters remain None;
/// unknown nested keys, including controller debug statistics, are ignored.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct IoStat {
  devices: BTreeMap<DeviceKey, IoDeviceStat>,
}

impl From<CoreIoStat> for IoStat {
  fn from(value: CoreIoStat) -> Self { Self { devices: IoValues::devices(value.devices(), |value| (*value).into()) } }
}

#[pymethods]
impl IoStat {
  /// Read-only mapping of (major, minor) keys to immutable device counters.
  #[getter]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    IoValues::mapping(py, &self.devices, |value| Py::new(py, value.clone()).map(Py::into_any))
  }
}

/// Immutable cumulative counters for one block device; absent values are None.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IoDeviceStat {
  /// Cumulative bytes read, or None when omitted.
  read_bytes: Option<u64>,
  /// Cumulative bytes written, or None when omitted.
  write_bytes: Option<u64>,
  /// Cumulative bytes discarded, or None when omitted by older kernels.
  discard_bytes: Option<u64>,
  /// Cumulative read operation count, or None when omitted.
  read_operations: Option<u64>,
  /// Cumulative write operation count, or None when omitted.
  write_operations: Option<u64>,
  /// Cumulative discard operation count, or None when omitted by older kernels.
  discard_operations: Option<u64>,
}

impl From<CoreIoDeviceStat> for IoDeviceStat {
  fn from(value: CoreIoDeviceStat) -> Self {
    Self {
      read_bytes: value.read_bytes().map(|value| value.get::<byte>()),
      write_bytes: value.write_bytes().map(|value| value.get::<byte>()),
      discard_bytes: value.discard_bytes().map(|value| value.get::<byte>()),
      read_operations: value.read_operations().map(|value| value.value),
      write_operations: value.write_operations().map(|value| value.value),
      discard_operations: value.discard_operations().map(|value| value.value),
    }
  }
}

/// Immutable snapshot of relative I/O weights, each from 1 through 10,000.
///
/// Only explicit device overrides are reported. Configuration can change
/// between reads; weights describe relative I/O time among siblings.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct IoWeight {
  /// Default scheduling weight applied to devices without an override.
  #[pyo3(get)]
  default_weight: u16,
  devices: BTreeMap<DeviceKey, u16>,
}

impl From<CoreIoWeight> for IoWeight {
  fn from(value: CoreIoWeight) -> Self {
    Self {
      default_weight: *value.default_weight().as_ref(),
      devices: IoValues::devices(value.devices(), |value| *value.as_ref()),
    }
  }
}

#[pymethods]
impl IoWeight {
  /// Read-only mapping of (major, minor) device keys to numeric weight overrides.
  #[getter]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, (&self.devices).into_py_dict(py)?.as_mapping()).unbind())
  }
}

/// Immutable configuration snapshot of per-device bandwidth and IOPS limits.
///
/// The map can be empty. MaxOr's max means unlimited here; ancestor limits
/// can still apply, and the kernel permits temporary bursts.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct IoMax {
  devices: BTreeMap<DeviceKey, IoDeviceMax>,
}

impl From<CoreIoMax> for IoMax {
  fn from(value: CoreIoMax) -> Self { Self { devices: IoValues::devices(value.devices(), |value| (*value).into()) } }
}

#[pymethods]
impl IoMax {
  /// Read-only mapping of (major, minor) keys to immutable per-device limits.
  #[getter]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    IoValues::mapping(py, &self.devices, |value| Py::new(py, value.clone()).map(Py::into_any))
  }
}

/// Immutable read and write bandwidth and operation-rate limits for one device.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Clone)]
pub(super) struct IoDeviceMax {
  /// Read bandwidth limit in bytes per second, or max.
  read_bps: MaxOr,
  /// Write bandwidth limit in bytes per second, or max.
  write_bps: MaxOr,
  /// Read operation limit in operations per second, or max.
  read_iops: MaxOr,
  /// Write operation limit in operations per second, or max.
  write_iops: MaxOr,
}

impl From<CoreIoDeviceMax> for IoDeviceMax {
  fn from(value: CoreIoDeviceMax) -> Self {
    Self {
      read_bps: MaxOr::from_value(value.read_bandwidth(), |value| {
        PythonLimitValue::Integer(value.get::<byte_per_second>())
      }),
      write_bps: MaxOr::from_value(value.write_bandwidth(), |value| {
        PythonLimitValue::Integer(value.get::<byte_per_second>())
      }),
      read_iops: MaxOr::from_value(value.read_iops(), |value| PythonLimitValue::Integer(value.get::<hertz>())),
      write_iops: MaxOr::from_value(value.write_iops(), |value| PythonLimitValue::Integer(value.get::<hertz>())),
    }
  }
}

/// Immutable configuration snapshot of per-device latency protection targets.
///
/// Targets are integer nanoseconds, not observed latency or guarantees. Only
/// configured nonzero targets appear; disabled targets are omitted. The map
/// can be empty, and targets can change between reads.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct IoLatency {
  targets_ns: BTreeMap<DeviceKey, u64>,
}

impl From<CoreIoLatency> for IoLatency {
  fn from(value: CoreIoLatency) -> Self {
    Self { targets_ns: IoValues::devices(value.devices(), |value| value.get::<nanosecond>()) }
  }
}

#[pymethods]
impl IoLatency {
  /// Read-only mapping of (major, minor) device keys to latency targets in nanoseconds.
  #[getter]
  fn targets_ns(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, self.targets_ns.clone().into_py_dict(py)?.as_mapping()).unbind())
  }
}

struct IoValues;

impl IoValues {
  fn devices<T, Output>(values: &BTreeMap<Device, T>, convert: impl Fn(&T) -> Output) -> BTreeMap<DeviceKey, Output> {
    values.iter().map(|(device, value)| ((device.major(), device.minor()), convert(value))).collect()
  }

  fn mapping<T>(
    py: Python<'_>,
    values: &BTreeMap<DeviceKey, T>,
    convert: impl Fn(&T) -> PyResult<Py<PyAny>>,
  ) -> PyResult<Py<PyMappingProxy>> {
    let dict = PyDict::new(py);
    for (device, value) in values {
      dict.set_item(device, convert(value)?)?;
    }
    Ok(PyMappingProxy::new(py, dict.as_mapping()).unbind())
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use pyo3::{
    exceptions::{PyAttributeError, PyTypeError, PyValueError},
    types::PyInt,
  };

  use super::*;

  impl IoDeviceStat {
    fn expected(bytes: [u64; 2], operations: [u64; 2], discard: Option<(u64, u64)>) -> Self {
      let [read_bytes, write_bytes] = bytes.map(Some);
      let [read_operations, write_operations] = operations.map(Some);
      Self {
        read_bytes,
        write_bytes,
        read_operations,
        write_operations,
        discard_bytes: discard.map(|(bytes, _)| bytes),
        discard_operations: discard.map(|(_, operations)| operations),
      }
    }
  }

  #[test]
  fn converts_io_counters_and_freezes_snapshots_and_device_maps() {
    Python::initialize();
    Python::attach(|py| {
      for (input, expected) in [
        (include_str!("../../../sakai/src/cgroup/v2/fixtures/p3/io.stat"), IoStat {
          devices: BTreeMap::from([
            ((8, 16), IoDeviceStat::expected([1_459_200, 314_773_504], [192, 353], Some((0, 0)))),
            ((8, 0), IoDeviceStat::expected([90_430_464, 299_008_000], [8950, 1252], Some((50_331_648, 3021)))),
          ]),
        }),
        (include_str!("../../../sakai/src/cgroup/v2/fixtures/p3_older/io.stat"), IoStat {
          devices: BTreeMap::from([
            ((8, 0), IoDeviceStat::expected([0, u64::MAX], [0, u64::MAX], None)),
            ((8, 16), IoDeviceStat {
              read_bytes: None,
              write_bytes: None,
              discard_bytes: None,
              read_operations: None,
              write_operations: None,
              discard_operations: None,
            }),
          ]),
        }),
        ("", IoStat { devices: BTreeMap::new() }),
      ] {
        let stat = IoStat::from(assert_ok!(input.parse::<CoreIoStat>()));
        assert_eq!(stat, expected);
        let object = assert_ok!(Py::new(py, stat));
        let devices = assert_ok!(object.bind(py).getattr("devices"));
        for (key, expected) in expected.devices {
          let device = assert_ok!(devices.get_item(key));
          for (name, expected) in [
            ("read_bytes", expected.read_bytes),
            ("write_bytes", expected.write_bytes),
            ("discard_bytes", expected.discard_bytes),
            ("read_operations", expected.read_operations),
            ("write_operations", expected.write_operations),
            ("discard_operations", expected.discard_operations),
          ] {
            assert_eq!(assert_ok!(assert_ok!(device.getattr(name)).extract::<Option<u64>>()), expected);
            assert!(assert_err!(device.setattr(name, 0)).is_instance_of::<PyAttributeError>(py));
          }
        }
        assert!(assert_err!(devices.set_item((8, 0), 0)).is_instance_of::<PyTypeError>(py));
        assert!(assert_err!(object.bind(py).setattr("devices", 0)).is_instance_of::<PyAttributeError>(py));
      }
    });
  }

  #[test]
  fn converts_default_and_device_weights_to_immutable_integer_maps() {
    Python::initialize();
    Python::attach(|py| {
      for (input, expected) in [
        (include_str!("../../../sakai/src/cgroup/v2/fixtures/p3/io.weight"), IoWeight {
          default_weight: 100,
          devices: BTreeMap::from([((8, 16), 200), ((8, 0), 50)]),
        }),
        (include_str!("../../../sakai/src/cgroup/v2/fixtures/p3_older/io.weight"), IoWeight {
          default_weight: 100,
          devices: BTreeMap::new(),
        }),
        ("default 1\n8:0 10000", IoWeight { default_weight: 1, devices: BTreeMap::from([((8, 0), 10_000)]) }),
      ] {
        let weight = IoWeight::from(assert_ok!(input.parse::<CoreIoWeight>()));
        assert_eq!(weight, expected);
        let object = assert_ok!(Py::new(py, weight));
        assert_eq!(
          assert_ok!(assert_ok!(object.bind(py).getattr("default_weight")).extract::<u16>()),
          expected.default_weight
        );
        let devices = assert_ok!(object.bind(py).getattr("devices"));
        let values = assert_ok!(py.get_type::<PyDict>().call1((&devices,)));
        assert_eq!(assert_ok!(values.extract::<BTreeMap<DeviceKey, u16>>()), expected.devices);
        assert!(assert_err!(devices.set_item((8, 0), 0)).is_instance_of::<PyTypeError>(py));
        for name in ["default_weight", "devices"] {
          assert!(assert_err!(object.bind(py).setattr(name, 0)).is_instance_of::<PyAttributeError>(py));
        }
      }
    });
  }

  #[test]
  fn converts_io_limits_without_losing_small_rates_or_unsigned_values() {
    Python::initialize();
    Python::attach(|py| {
      for (input, expected) in [
        (
          include_str!("../../../sakai/src/cgroup/v2/fixtures/p3/io.max"),
          BTreeMap::from([
            ((8, 16), [Some(2_097_152_u64), None, None, Some(120)]),
            ((8, 0), [Some(1), Some(u64::MAX), Some(1), Some(u64::MAX)]),
          ]),
        ),
        (
          include_str!("../../../sakai/src/cgroup/v2/fixtures/p3_older/io.max"),
          BTreeMap::from([((8, 0), [None, None, None, None])]),
        ),
        ("8:0 rbps=0 wbps=0 riops=0 wiops=0", BTreeMap::from([((8, 0), [Some(0), Some(0), Some(0), Some(0)])])),
        ("", BTreeMap::new()),
      ] {
        let snapshot = IoMax::from(assert_ok!(input.parse::<CoreIoMax>()));
        let object = assert_ok!(Py::new(py, snapshot));
        let devices = assert_ok!(object.bind(py).getattr("devices"));
        assert_eq!(assert_ok!(devices.len()), expected.len());
        for (key, expected) in expected {
          let device = assert_ok!(devices.get_item(key));
          for (name, expected) in ["read_bps", "write_bps", "read_iops", "write_iops"].into_iter().zip(expected) {
            let limit = assert_ok!(device.getattr(name));
            assert_eq!(assert_ok!(assert_ok!(limit.getattr("is_max")).extract::<bool>()), expected.is_none());
            match expected {
              | Some(expected) => {
                let value = assert_ok!(limit.getattr("value"));
                assert!(value.is_instance_of::<PyInt>());
                assert_eq!(assert_ok!(value.extract::<u64>()), expected);
              },
              | None => assert!(assert_err!(limit.getattr("value")).is_instance_of::<PyValueError>(py)),
            }
            assert!(assert_err!(limit.setattr("value", 0)).is_instance_of::<PyAttributeError>(py));
            assert!(assert_err!(device.setattr(name, 0)).is_instance_of::<PyAttributeError>(py));
          }
        }
        assert!(assert_err!(devices.set_item((8, 0), 0)).is_instance_of::<PyTypeError>(py));
        assert!(assert_err!(object.bind(py).setattr("devices", 0)).is_instance_of::<PyAttributeError>(py));
      }
    });
  }

  #[test]
  fn converts_latency_targets_to_immutable_nanosecond_maps() {
    Python::initialize();
    Python::attach(|py| {
      for (input, expected) in [
        (include_str!("../../../sakai/src/cgroup/v2/fixtures/p3/io.latency"), IoLatency {
          targets_ns: BTreeMap::from([((8, 16), 25_000_000), ((8, 0), 1000)]),
        }),
        ("", IoLatency { targets_ns: BTreeMap::new() }),
      ] {
        let latency = IoLatency::from(assert_ok!(input.parse::<CoreIoLatency>()));
        assert_eq!(latency, expected);
        let object = assert_ok!(Py::new(py, latency));
        let targets = assert_ok!(object.bind(py).getattr("targets_ns"));
        let values = assert_ok!(py.get_type::<PyDict>().call1((&targets,)));
        assert_eq!(assert_ok!(values.extract::<BTreeMap<DeviceKey, u64>>()), expected.targets_ns);
        assert!(assert_err!(targets.set_item((8, 0), 0)).is_instance_of::<PyTypeError>(py));
        assert!(assert_err!(object.bind(py).setattr("targets_ns", 0)).is_instance_of::<PyAttributeError>(py));
      }
    });
  }
}
