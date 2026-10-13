//! Python snapshots and readers for the cgroup v2 I/O controller.

use std::{collections::BTreeMap, sync::Arc};

#[cfg(test)]
use pyo3::types::PyDict;
use pyo3::{
  prelude::*,
  types::{IntoPyDict, PyMappingProxy},
};
use sakai::v2::io::{
  Device, IoCostModel as CoreIoCostModel, IoCostQos as CoreIoCostQos, IoDeviceCostModel as CoreIoDeviceCostModel,
  IoDeviceCostQos as CoreIoDeviceCostQos, IoDeviceMax as CoreIoDeviceMax, IoDeviceStat as CoreIoDeviceStat,
  IoLatency as CoreIoLatency, IoMax as CoreIoMax, IoStat as CoreIoStat, IoWeight as CoreIoWeight,
};
use uom::si::{frequency::hertz, information::byte, information_rate::byte_per_second, ratio::ratio, time::nanosecond};

use super::{MaxOr, Pressure, PythonError, limit::PythonLimitValue, mapping::mapping};

type DeviceKey = (u32, u32);

/// Fresh I/O reads for one pinned cgroup; each call is a separate snapshot.
///
/// Missing files, including optional kernel features and root exemptions,
/// raise InterfaceMissingError with the interface path. Reads never write PSI
/// triggers or change controller configuration.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct IoReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
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

  /// Read root-only I/O cost QoS parameters; percentages are dimensionless ratios.
  /// Requires CONFIG_BLK_CGROUP_IOCOST; automatic parameters may change between reads.
  fn cost_qos(&self, py: Python<'_>) -> PyResult<IoCostQos> {
    PythonError::read(py, &self.owner, |owner| owner.io().cost_qos()).map(Into::into)
  }

  /// Read root-only I/O cost models; rates are model inputs, not measured usage.
  /// Requires CONFIG_BLK_CGROUP_IOCOST; automatic parameters may change between reads.
  fn cost_model(&self, py: Python<'_>) -> PyResult<IoCostModel> {
    PythonError::read(py, &self.owner, |owner| owner.io().cost_model()).map(Into::into)
  }

  /// Read the I/O request-priority policy, including at root.
  /// Requires CONFIG_BLK_CGROUP_IOPRIO. Legacy none-to-rt has the same behavior
  /// as promote-to-rt since Linux 6.5; earlier kernels only promote unclassified I/O.
  #[gen_stub(override_return_type(type_repr = "typing.Literal['no-change', 'promote-to-rt', 'none-to-rt', 'restrict-to-be', 'idle']", imports = ("typing",)))]
  fn priority_class(&self, py: Python<'_>) -> PyResult<String> {
    PythonError::read(py, &self.owner, |owner| owner.io().priority_class()).map(|value| value.to_string())
  }
}

/// Immutable, volatile I/O counters keyed by (major, minor) device tuples.
///
/// An empty map means no devices were reported. Missing counters remain None;
/// latency debug fields describe the device group; unknown nested keys are ignored.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq)]
pub(super) struct IoStat {
  devices: BTreeMap<DeviceKey, IoDeviceStat>,
}

impl From<CoreIoStat> for IoStat {
  fn from(value: CoreIoStat) -> Self { Self { devices: IoValues::devices(value.devices(), |value| (*value).into()) } }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl IoStat {
  /// Read-only mapping of (major, minor) keys to immutable device counters.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[tuple[int, int], IoDeviceStat]", imports = ("collections.abc",)))]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.devices, |value| Py::new(py, value.clone()))
  }
}

/// Immutable cumulative counters for one block device; absent values are None.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct IoDeviceStat {
  /// Cumulative bytes read, or None when omitted.
  #[pyo3(get)]
  read_bytes: Option<u64>,
  /// Cumulative bytes written, or None when omitted.
  #[pyo3(get)]
  write_bytes: Option<u64>,
  /// Cumulative bytes discarded, or None when omitted by older kernels.
  #[pyo3(get)]
  discard_bytes: Option<u64>,
  /// Cumulative read operation count, or None when omitted.
  #[pyo3(get)]
  read_operations: Option<u64>,
  /// Cumulative write operation count, or None when omitted.
  #[pyo3(get)]
  write_operations: Option<u64>,
  /// Cumulative discard operation count, or None when omitted by older kernels.
  #[pyo3(get)]
  discard_operations: Option<u64>,
  /// Delay-control indicator; -1 means fixed delay, nonnegative values count scaled-delay users.
  #[pyo3(get)]
  use_delay: Option<i32>,
  /// Current artificial delay in nanoseconds, not cumulative stall time.
  #[pyo3(get)]
  delay_ns: Option<u64>,
  /// Queue-depth limit, or max when unrestricted; None means the field was omitted.
  depth: Option<sakai::MaxOr<sakai::Count>>,
  /// Rotational-device exponential moving average latency in nanoseconds.
  #[pyo3(get)]
  average_latency_ns: Option<u64>,
  /// Rotational-device sampling window in nanoseconds, converted from milliseconds.
  #[pyo3(get)]
  latency_window_ns: Option<u64>,
  /// Non-rotational I/Os exceeding their target in the current window; can reset.
  #[pyo3(get)]
  missed: Option<u64>,
  /// Non-rotational I/Os accounted in the current window; can reset.
  #[pyo3(get)]
  total: Option<u64>,
  /// Cumulative hierarchical modeled I/O usage in nanoseconds, distinct from actual service time.
  #[pyo3(get)]
  cost_usage_ns: Option<u64>,
  /// Cumulative hierarchical I/O cost wait time in nanoseconds, when debug statistics are enabled.
  #[pyo3(get)]
  cost_wait_ns: Option<u64>,
  /// Cumulative hierarchical time in I/O cost debt in nanoseconds, when debug statistics are enabled.
  #[pyo3(get)]
  cost_indebt_ns: Option<u64>,
  /// Cumulative hierarchical I/O cost delay time in nanoseconds, when debug statistics are enabled.
  #[pyo3(get)]
  cost_indelay_ns: Option<u64>,
  /// Root-only device virtual-time rate as a ratio; kernel 150% becomes 1.5.
  #[pyo3(get)]
  cost_vrate: Option<f64>,
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
      use_delay: value.delay_usage().map(|value| value.into_inner()),
      delay_ns: value.delay().map(|value| value.get::<nanosecond>()),
      depth: value.depth(),
      average_latency_ns: value.average_latency().map(|value| value.get::<nanosecond>()),
      latency_window_ns: value.latency_window().map(|value| value.get::<nanosecond>()),
      missed: value.missed().map(|value| value.value),
      total: value.total().map(|value| value.value),
      cost_usage_ns: value.cost_usage().map(|value| value.get::<nanosecond>()),
      cost_wait_ns: value.cost_wait().map(|value| value.get::<nanosecond>()),
      cost_indebt_ns: value.cost_indebt().map(|value| value.get::<nanosecond>()),
      cost_indelay_ns: value.cost_indelay().map(|value| value.get::<nanosecond>()),
      cost_vrate: value.cost_vrate().map(|value| value.get::<ratio>()),
    }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl IoDeviceStat {
  /// Queue-depth limit, or max when unrestricted; None means the field was omitted.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "MaxOr[int] | None"))]
  fn depth(&self) -> Option<MaxOr> {
    self.depth.map(|value| MaxOr::from_value(value, |value| PythonLimitValue::Integer(value.value)))
  }
}

/// Immutable snapshot of relative I/O weights, each from 1 through 10,000.
///
/// Only explicit device overrides are reported. Configuration can change
/// between reads; weights describe relative I/O time among siblings.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
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

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl IoWeight {
  /// Read-only mapping of (major, minor) device keys to numeric weight overrides.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[tuple[int, int], int]", imports = ("collections.abc",)))]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, (&self.devices).into_py_dict(py)?.as_mapping()).unbind())
  }
}

/// Immutable configuration snapshot of per-device bandwidth and IOPS limits.
///
/// The map can be empty. MaxOr's max means unlimited here; ancestor limits
/// can still apply, and the kernel permits temporary bursts.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct IoMax {
  devices: BTreeMap<DeviceKey, IoDeviceMax>,
}

impl From<CoreIoMax> for IoMax {
  fn from(value: CoreIoMax) -> Self { Self { devices: IoValues::devices(value.devices(), |value| (*value).into()) } }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl IoMax {
  /// Read-only mapping of (major, minor) keys to immutable per-device limits.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[tuple[int, int], IoDeviceMax]", imports = ("collections.abc",)))]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.devices, |value| Py::new(py, value.clone()))
  }
}

/// Immutable read and write bandwidth and operation-rate limits for one device.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
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
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct IoLatency {
  targets_ns: BTreeMap<DeviceKey, u64>,
}

impl From<CoreIoLatency> for IoLatency {
  fn from(value: CoreIoLatency) -> Self {
    Self { targets_ns: IoValues::devices(value.devices(), |value| value.as_ref().get::<nanosecond>()) }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl IoLatency {
  /// Read-only mapping of (major, minor) device keys to latency targets in nanoseconds.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[tuple[int, int], int]", imports = ("collections.abc",)))]
  fn targets_ns(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, self.targets_ns.clone().into_py_dict(py)?.as_mapping()).unbind())
  }
}

/// Immutable root-only per-device I/O cost quality-of-service configuration.
///
/// Requires CONFIG_BLK_CGROUP_IOCOST. Device maps can be empty; automatic
/// parameters may change between reads. This snapshot performs no kernel I/O.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct IoCostQos {
  devices: BTreeMap<DeviceKey, IoDeviceCostQos>,
}

impl From<CoreIoCostQos> for IoCostQos {
  fn from(value: CoreIoCostQos) -> Self {
    Self { devices: IoValues::devices(value.devices(), |value| (*value).into()) }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl IoCostQos {
  /// Read-only mapping of (major, minor) device keys to QoS parameters.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[tuple[int, int], IoDeviceCostQos]", imports = ("collections.abc",)))]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.devices, |value| Py::new(py, value.clone()))
  }
}

/// Immutable I/O cost enablement, latency targets, and scaling bounds for one device.
///
/// Ratios represent the kernel's percentages: 150% is 1.5. Latency thresholds
/// use nanoseconds. Parameters describe configuration, not measured throughput.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Clone)]
pub(super) struct IoDeviceCostQos {
  /// Whether weight-based I/O cost control is enabled for the device.
  enabled: bool,
  /// Parameter selection mode: auto or user.
  control: String,
  /// Read completion latency percentile, from zero to one; zero disables its use.
  read_percentile: f64,
  /// Read latency threshold in nanoseconds.
  read_latency_ns: u64,
  /// Write completion latency percentile, from zero to one; zero disables its use.
  write_percentile: f64,
  /// Write latency threshold in nanoseconds.
  write_latency_ns: u64,
  /// Minimum I/O issue-rate scaling, from 0.01 through 100.
  min_scaling: f64,
  /// Maximum I/O issue-rate scaling, from 0.01 through 100.
  max_scaling: f64,
}

impl From<CoreIoDeviceCostQos> for IoDeviceCostQos {
  fn from(value: CoreIoDeviceCostQos) -> Self {
    Self {
      enabled: value.enabled(),
      control: value.control().to_string(),
      read_percentile: value.read_percentile().get::<ratio>(),
      read_latency_ns: value.read_latency().get::<nanosecond>(),
      write_percentile: value.write_percentile().get::<ratio>(),
      write_latency_ns: value.write_latency().get::<nanosecond>(),
      min_scaling: value.min_scaling().get::<ratio>(),
      max_scaling: value.max_scaling().get::<ratio>(),
    }
  }
}

/// Immutable root-only per-device I/O cost model configuration.
///
/// Requires CONFIG_BLK_CGROUP_IOCOST. Device maps can be empty; automatic
/// parameters may change between reads. This snapshot performs no kernel I/O.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct IoCostModel {
  devices: BTreeMap<DeviceKey, IoDeviceCostModel>,
}

impl From<CoreIoCostModel> for IoCostModel {
  fn from(value: CoreIoCostModel) -> Self {
    Self { devices: IoValues::devices(value.devices(), |value| (*value).into()) }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl IoCostModel {
  /// Read-only mapping of (major, minor) device keys to cost model parameters.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[tuple[int, int], IoDeviceCostModel]", imports = ("collections.abc",)))]
  fn devices(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    mapping(py, &self.devices, |value| Py::new(py, value.clone()))
  }
}

/// Immutable I/O cost model rates for one device, not observed or enforced rates.
///
/// Sequential and random IOPS describe 4 KiB I/O operations per second.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Clone)]
pub(super) struct IoDeviceCostModel {
  /// Parameter selection mode: auto or user.
  control: String,
  /// Model name: linear.
  model: String,
  /// Model's maximum sequential read throughput in bytes per second.
  read_bps: u64,
  /// Model's maximum sequential 4 KiB read operations per second.
  read_sequential_iops: u64,
  /// Model's maximum random 4 KiB read operations per second.
  read_random_iops: u64,
  /// Model's maximum sequential write throughput in bytes per second.
  write_bps: u64,
  /// Model's maximum sequential 4 KiB write operations per second.
  write_sequential_iops: u64,
  /// Model's maximum random 4 KiB write operations per second.
  write_random_iops: u64,
}

impl From<CoreIoDeviceCostModel> for IoDeviceCostModel {
  fn from(value: CoreIoDeviceCostModel) -> Self {
    Self {
      control: value.control().to_string(),
      model: value.model().to_string(),
      read_bps: value.read_bandwidth().get::<byte_per_second>(),
      read_sequential_iops: value.read_sequential_iops().get::<hertz>(),
      read_random_iops: value.read_random_iops().get::<hertz>(),
      write_bps: value.write_bandwidth().get::<byte_per_second>(),
      write_sequential_iops: value.write_sequential_iops().get::<hertz>(),
      write_random_iops: value.write_random_iops().get::<hertz>(),
    }
  }
}

struct IoValues;

impl IoValues {
  fn devices<T, Output>(values: &BTreeMap<Device, T>, convert: impl Fn(&T) -> Output) -> BTreeMap<DeviceKey, Output> {
    values.iter().map(|(device, value)| ((device.major(), device.minor()), convert(value))).collect()
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_in_delta, assert_ok};

  use super::{
    super::tests::{check_frozen, check_limit, readonly},
    *,
  };

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
        ..Self::default()
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
              use_delay: Some(1),
              delay_ns: Some(12_345),
              depth: Some(sakai::MaxOr::Value(sakai::Count { value: 1, ..Default::default() })),
              average_latency_ns: Some(100_000),
              latency_window_ns: Some(100_000_000),
              ..IoDeviceStat::default()
            }),
          ]),
        }),
        ("", IoStat { devices: BTreeMap::new() }),
      ] {
        let stat = IoStat::from(assert_ok!(input.parse::<CoreIoStat>()));
        assert_eq!(stat, expected);
        let object = assert_ok!(Py::new(py, stat));
        let devices = readonly(object.bind(py).as_any(), "devices", (8, 0));
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
            check_frozen(&device, &[name]);
          }
        }
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
        let devices = readonly(object.bind(py).as_any(), "devices", (8, 0));
        let values = assert_ok!(py.get_type::<PyDict>().call1((&devices,)));
        assert_eq!(assert_ok!(values.extract::<BTreeMap<DeviceKey, u16>>()), expected.devices);
        check_frozen(object.bind(py).as_any(), &["default_weight"]);
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
        let devices = readonly(object.bind(py).as_any(), "devices", (8, 0));
        assert_eq!(assert_ok!(devices.len()), expected.len());
        for (key, expected) in expected {
          let device = assert_ok!(devices.get_item(key));
          for (name, expected) in ["read_bps", "write_bps", "read_iops", "write_iops"].into_iter().zip(expected) {
            let limit = assert_ok!(device.getattr(name));
            check_limit(&limit, expected);
            check_frozen(&limit, &["value"]);
            check_frozen(&device, &[name]);
          }
        }
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
        let targets = readonly(object.bind(py).as_any(), "targets_ns", (8, 0));
        let values = assert_ok!(py.get_type::<PyDict>().call1((&targets,)));
        assert_eq!(assert_ok!(values.extract::<BTreeMap<DeviceKey, u64>>()), expected.targets_ns);
      }
    });
  }

  #[test]
  fn converts_latency_and_cost_statistics_without_mixing_units_or_missing_values() {
    let input = "8:0 use_delay=-1 delay_nsec=9 depth=max avg_lat=2 win=3 missed=4 total=5 cost.usage=6 cost.wait=7 \
                 cost.indebt=8 cost.indelay=9 cost.vrate=150.00";
    let stat = IoStat::from(assert_ok!(input.parse::<CoreIoStat>()));
    let expected = IoDeviceStat {
      use_delay: Some(-1),
      delay_ns: Some(9),
      depth: Some(sakai::MaxOr::Max),
      average_latency_ns: Some(2_000),
      latency_window_ns: Some(3_000_000),
      missed: Some(4),
      total: Some(5),
      cost_usage_ns: Some(6_000),
      cost_wait_ns: Some(7_000),
      cost_indebt_ns: Some(8_000),
      cost_indelay_ns: Some(9_000),
      cost_vrate: Some(1.5),
      ..IoDeviceStat::default()
    };
    assert_eq!(stat, IoStat { devices: BTreeMap::from([((8, 0), expected)]) });
  }

  #[test]
  fn converts_cost_configuration_into_immutable_typed_device_maps() {
    Python::initialize();
    Python::attach(|py| {
      let qos = IoCostQos::from(assert_ok!(
        "8:0 enable=1 ctrl=user rpct=95 rlat=2 wpct=0 wlat=0 min=50 max=10000".parse::<CoreIoCostQos>()
      ));
      let qos = assert_ok!(Py::new(py, qos));
      let devices = readonly(qos.bind(py).as_any(), "devices", (8, 0));
      let device = assert_ok!(devices.get_item((8, 0)));
      assert!(assert_ok!(assert_ok!(device.getattr("enabled")).extract::<bool>()));
      assert_eq!(assert_ok!(assert_ok!(device.getattr("control")).extract::<String>()), "user");
      assert_eq!(assert_ok!(assert_ok!(device.getattr("read_latency_ns")).extract::<u64>()), 2_000);
      for (name, expected) in [("read_percentile", 0.95_f64), ("min_scaling", 0.5), ("max_scaling", 100.0)] {
        assert_in_delta!(
          assert_ok!(assert_ok!(device.getattr(name)).extract::<f64>()),
          expected,
          f64::EPSILON * expected.max(1.0)
        );
        check_frozen(&device, &[name]);
      }

      let model = IoCostModel::from(assert_ok!(
        "8:0 ctrl=auto model=linear rbps=1 rseqiops=2 rrandiops=3 wbps=18446744073709551615 wseqiops=5 wrandiops=6"
          .parse::<CoreIoCostModel>()
      ));
      let model = assert_ok!(Py::new(py, model));
      let devices = readonly(model.bind(py).as_any(), "devices", (8, 0));
      let device = assert_ok!(devices.get_item((8, 0)));
      assert_eq!(assert_ok!(assert_ok!(device.getattr("control")).extract::<String>()), "auto");
      assert_eq!(assert_ok!(assert_ok!(device.getattr("model")).extract::<String>()), "linear");
      for (name, expected) in [
        ("read_bps", 1),
        ("read_sequential_iops", 2),
        ("read_random_iops", 3),
        ("write_bps", u64::MAX),
        ("write_sequential_iops", 5),
        ("write_random_iops", 6),
      ] {
        assert_eq!(assert_ok!(assert_ok!(device.getattr(name)).extract::<u64>()), expected);
        check_frozen(&device, &[name]);
      }
    });
  }
}
