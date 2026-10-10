//! Linux cgroup v2 Python bindings.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use pyo3::{
  PyTypeInfo, create_exception,
  exceptions::{PyException, PyOSError, PyValueError},
  prelude::*,
  types::{IntoPyDict, PyDict, PyMappingProxy, PyModule},
};
use sakai::{
  Bytes, Count, Error, MaxOr as CoreMaxOr, NonZeroTime, Pressure as CorePressure, PressureLine as CorePressureLine,
  Ratio,
  v2::{
    core::{CgroupEvents as CoreCgroupEvents, CgroupStat as CoreCgroupStat},
    cpu::{
      CpuBandwidthStat as CoreCpuBandwidthStat, CpuBurstStat as CoreCpuBurstStat, CpuMax as CoreCpuMax,
      CpuStat as CoreCpuStat, CpuStatLocal as CoreCpuStatLocal, CpuTimeStat as CoreCpuTimeStat,
      CpuWeight as CoreCpuWeight,
    },
    memory::{
      MemoryEventCounts as CoreMemoryEventCounts, MemoryNumaStat as CoreMemoryNumaStat, MemoryStat as CoreMemoryStat,
      SwapEvents as CoreSwapEvents,
    },
    pids::PidsEvents as CorePidsEvents,
  },
};
use uom::si::{information::byte, ratio::ratio, time::nanosecond};

create_exception!(_sakai, SakaiError, PyException, "Base error for Sakai cgroup operations.");
create_exception!(
  _sakai,
  InterfaceMissingError,
  SakaiError,
  "A cgroup interface file is missing; path identifies the interface."
);
create_exception!(_sakai, NotCgroupV2Error, SakaiError, "The requested path is not on a cgroup v2 filesystem.");
create_exception!(
  _sakai,
  DeletedCgroupError,
  SakaiError,
  "The pinned cgroup has been deleted; path identifies its former location."
);
create_exception!(
  _sakai,
  CgroupParseError,
  SakaiError,
  "A cgroup interface could not be parsed; path identifies the interface."
);
create_exception!(_sakai, NotSupportedError, SakaiError, "The operation is not supported for this cgroup.");

struct PythonError;

struct PythonPath;

impl PythonPath {
  fn extract<T>(object: &Bound<'_, T>) -> PyResult<PathBuf> {
    // fsdecode first applies os.fspath and decodes bytes with surrogateescape.
    // PyO3 0.29's PathBuf extractor accepts only str results from os.fspath.
    object.py().import("os")?.call_method1("fsdecode", (object.as_any(),))?.extract()
  }
}

impl PythonError {
  fn with_path<E: PyTypeInfo>(py: Python<'_>, message: String, path: PathBuf) -> PyErr {
    let error = PyErr::new::<E, _>(message);
    // A path is diagnostic. PyO3 uses os.fspath-compatible, lossless path conversion.
    if let Err(attribute_error) = error.value(py).setattr("path", path) {
      return attribute_error;
    }
    error
  }

  fn from_core(py: Python<'_>, error: Error) -> PyErr {
    match error {
      | Error::FileMissing { path } => {
        Self::with_path::<InterfaceMissingError>(py, format!("cgroup interface is missing: {}", path.display()), path)
      },
      | Error::NotSupported => NotSupportedError::new_err("operation is not supported for this cgroup"),
      | Error::NotCgroupV2 => NotCgroupV2Error::new_err("not a cgroup v2 filesystem"),
      | Error::DeletedCgroup { path } => {
        Self::with_path::<DeletedCgroupError>(py, format!("cgroup has been deleted: {}", path.display()), path)
      },
      | Error::Parse { path, source } => {
        Self::with_path::<CgroupParseError>(py, format!("failed to parse {}: {source}", path.display()), path)
      },
      // Construct OSError first so errno selects its standard subclass even
      // on Python 3.11, where a lazy PyErr can retain OSError as its type.
      | Error::Io(source) => match source.raw_os_error() {
        | Some(errno) => match py.get_type::<PyOSError>().call1((errno, source.to_string())) {
          | Ok(exception) => PyErr::from_value(exception),
          | Err(error) => error,
        },
        | None => PyOSError::new_err(source.to_string()),
      },
    }
  }

  fn result<T>(py: Python<'_>, result: Result<T, Error>) -> PyResult<T> {
    result.map_err(|error| Self::from_core(py, error))
  }

  fn read<T: Send + 'static>(
    py: Python<'_>,
    owner: &Arc<sakai::Cgroup>,
    read: impl FnOnce(&sakai::Cgroup) -> Result<T, Error> + Send + 'static,
  ) -> PyResult<T> {
    // Detach only after owning all inputs; shared handles never depend on the GIL.
    // Construct Python values and translate exceptions after reattaching.
    let owner = Arc::clone(owner);
    Self::result(py, py.detach(move || read(&owner)))
  }
}

/// An open, pinned cgroup v2 directory handle.
///
/// Readers retain this handle's ownership, so they remain usable after the
/// Python Cgroup object is released. Each read fetches a fresh kernel value;
/// separate calls do not form an atomic snapshot.
#[pyclass(frozen, module = "sakai._sakai")]
struct Cgroup {
  owner: Arc<sakai::Cgroup>,
}

impl From<sakai::Cgroup> for Cgroup {
  fn from(owner: sakai::Cgroup) -> Self { Self { owner: Arc::new(owner) } }
}

#[pymethods]
impl Cgroup {
  /// Open the cgroup containing the current process.
  #[staticmethod]
  fn current(py: Python<'_>) -> PyResult<Self> {
    PythonError::result(py, py.detach(sakai::Cgroup::from_current_process)).map(Into::into)
  }

  /// Open the cgroup containing a process ID.
  #[staticmethod]
  fn from_pid(py: Python<'_>, pid: i64) -> PyResult<Self> {
    let pid = u32::try_from(pid)?;
    PythonError::result(py, py.detach(move || sakai::Cgroup::from_pid(pid))).map(Into::into)
  }

  /// Open a cgroup v2 directory by its filesystem path.
  ///
  /// Accepts str, bytes, and os.PathLike without lossy path conversion.
  #[staticmethod]
  fn from_path(py: Python<'_>, #[pyo3(from_py_with = PythonPath::extract)] path: PathBuf) -> PyResult<Self> {
    PythonError::result(py, py.detach(move || sakai::Cgroup::from_path(&path))).map(Into::into)
  }

  /// Diagnostic filesystem path; it may become stale after a rename or deletion.
  /// Reads use the pinned directory handle, never this display path.
  #[getter]
  fn path(&self) -> PathBuf { self.owner.path().to_owned() }

  /// Open a direct child by one filesystem name, without following slashes.
  fn child(&self, py: Python<'_>, #[pyo3(from_py_with = PythonPath::extract)] name: PathBuf) -> PyResult<Self> {
    PythonError::read(py, &self.owner, move |owner| owner.child(name.as_os_str())).map(Into::into)
  }

  /// Return separate pinned handles for the current direct children.
  fn children(&self, py: Python<'_>) -> PyResult<Vec<Self>> {
    PythonError::read(py, &self.owner, sakai::Cgroup::children)
      .map(|children| children.into_iter().map(Into::into).collect())
  }

  /// Return a reader for CPU controller interfaces.
  fn cpu(&self) -> CpuReader { CpuReader { owner: Arc::clone(&self.owner) } }

  /// Return a reader for memory controller interfaces.
  fn memory(&self) -> MemoryReader { MemoryReader { owner: Arc::clone(&self.owner) } }

  /// Return a process-number reader; kernel PID counts include threads.
  fn pids(&self) -> PidsReader { PidsReader { owner: Arc::clone(&self.owner) } }

  /// Return a reader for core cgroup topology interfaces.
  fn core(&self) -> CoreReader { CoreReader { owner: Arc::clone(&self.owner) } }

  fn __repr__(&self) -> String { format!("Cgroup({:?})", self.owner.path()) }
}

#[derive(Debug, Clone, Copy, pyo3::IntoPyObject)]
enum PythonLimitValue {
  Integer(u64),
  Ratio(f64),
}

/// A kernel `max` or a concrete value in the surrounding field's units.
///
/// This class is generic in Python: integer limits use `MaxOr[int]` and
/// dimensionless ratios use `MaxOr[float]`. Check `is_max` before reading
/// `value`; accessing `value` for `max` raises ValueError.
#[pyclass(generic, frozen, skip_from_py_object, module = "sakai._sakai")]
#[derive(Clone, Copy)]
struct MaxOr {
  inner: Option<PythonLimitValue>,
}

impl MaxOr {
  fn from_value<T>(value: CoreMaxOr<T>, convert: impl FnOnce(T) -> PythonLimitValue) -> Self {
    Self {
      inner: match value {
        | CoreMaxOr::Max => None,
        | CoreMaxOr::Value(value) => Some(convert(value)),
      },
    }
  }

  fn from_time(value: CoreMaxOr<NonZeroTime>) -> Self {
    Self::from_value(value, |time| PythonLimitValue::Integer(time.get::<nanosecond>()))
  }

  fn from_bytes(value: CoreMaxOr<Bytes>) -> Self {
    Self::from_value(value, |bytes| PythonLimitValue::Integer(bytes.get::<byte>()))
  }

  fn from_ratio(value: CoreMaxOr<Ratio>) -> Self {
    Self::from_value(value, |value| PythonLimitValue::Ratio(value.get::<ratio>()))
  }

  fn from_count(value: CoreMaxOr<Count>) -> Self {
    Self::from_value(value, |count| PythonLimitValue::Integer(count.value))
  }
}

#[pymethods]
impl MaxOr {
  /// Whether the kernel reported `max`.
  #[getter]
  fn is_max(&self) -> bool { self.inner.is_none() }

  /// Concrete value; raises ValueError when the kernel reported `max`.
  #[getter]
  fn value(&self) -> PyResult<PythonLimitValue> {
    self.inner.ok_or_else(|| PyValueError::new_err("max has no numeric value"))
  }

  fn __repr__(&self) -> String {
    match self.inner {
      | None => "MaxOr(max)".into(),
      | Some(PythonLimitValue::Integer(value)) => format!("MaxOr(value={value})"),
      | Some(PythonLimitValue::Ratio(value)) => format!("MaxOr(value={value})"),
    }
  }
}

/// Fresh reads of CPU controller files for one pinned cgroup.
#[pyclass(frozen, module = "sakai._sakai")]
struct CpuReader {
  owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl CpuReader {
  /// Read hierarchical CPU usage and optional bandwidth counters.
  fn stat(&self, py: Python<'_>) -> PyResult<CpuStat> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().stat()).map(Into::into)
  }

  /// Read local CPU throttling counters, if the interface exists.
  fn stat_local(&self, py: Python<'_>) -> PyResult<CpuStatLocal> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().stat_local()).map(Into::into)
  }

  /// Read this cgroup's quota and period, not effective ancestor capacity.
  fn max(&self, py: Python<'_>) -> PyResult<CpuMax> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().max()).map(Into::into)
  }

  /// Read CPU weight, preserving the special idle state.
  fn weight(&self, py: Python<'_>) -> PyResult<CpuWeight> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().weight()).map(Into::into)
  }

  /// Read CPU weight as a nice value from -20 through 19.
  fn weight_nice(&self, py: Python<'_>) -> PyResult<i8> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().weight_nice()).map(|nice| *nice.as_ref())
  }

  /// Read the allowed CPU bandwidth burst in nanoseconds.
  fn max_burst(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().max_burst()).map(|burst| burst.value().get::<nanosecond>())
  }

  /// Read whether this cgroup is configured for idle CPU scheduling.
  fn idle(&self, py: Python<'_>) -> PyResult<bool> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().idle()).map(|idle| idle.value())
  }

  /// Read the minimum utilization clamp as a ratio from zero to one.
  fn uclamp_min(&self, py: Python<'_>) -> PyResult<f64> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().uclamp_min()).map(|clamp| clamp.value().get::<ratio>())
  }

  /// Read the maximum utilization clamp as `max` or a concrete ratio.
  fn uclamp_max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().uclamp_max()).map(|clamp| MaxOr::from_ratio(clamp.value()))
  }

  /// Read CPU pressure-stall averages and cumulative times.
  fn pressure(&self, py: Python<'_>) -> PyResult<Pressure> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().pressure()).map(Into::into)
  }
}

/// Hierarchical CPU usage and optional bandwidth statistics.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
struct CpuStat {
  /// CPU usage split into total, user, and system nanoseconds.
  time: CpuTimeStat,
  /// Bandwidth counters, or None when the kernel omitted them.
  bandwidth: Option<CpuBandwidthStat>,
}

impl From<CoreCpuStat> for CpuStat {
  fn from(value: CoreCpuStat) -> Self {
    Self { time: value.time().into(), bandwidth: value.bandwidth().map(Into::into) }
  }
}

/// Local CPU statistics, distinct from hierarchical cpu.stat.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
struct CpuStatLocal {
  /// Locally throttled time in nanoseconds, if reported by the kernel.
  throttled_ns: Option<u64>,
}

impl From<CoreCpuStatLocal> for CpuStatLocal {
  fn from(value: CoreCpuStatLocal) -> Self {
    Self { throttled_ns: value.throttled().map(|time| time.get::<nanosecond>()) }
  }
}

/// CPU usage times in nanoseconds.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
struct CpuTimeStat {
  /// Total CPU usage in nanoseconds.
  usage_ns: u64,
  /// CPU usage in user mode, in nanoseconds.
  user_ns: u64,
  /// CPU usage in kernel mode, in nanoseconds.
  system_ns: u64,
}

impl From<CoreCpuTimeStat> for CpuTimeStat {
  fn from(value: CoreCpuTimeStat) -> Self {
    Self {
      usage_ns: value.usage().get::<nanosecond>(),
      user_ns: value.user().get::<nanosecond>(),
      system_ns: value.system().get::<nanosecond>(),
    }
  }
}

/// CPU bandwidth periods, throttling, and optional burst counters.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
struct CpuBandwidthStat {
  /// Number of elapsed bandwidth periods.
  nr_periods: u64,
  /// Number of bandwidth periods in which the cgroup was throttled.
  nr_throttled: u64,
  /// Cumulative throttled time in nanoseconds.
  throttled_ns: u64,
  /// Burst counters, if the kernel reports them.
  burst: Option<CpuBurstStat>,
}

impl From<CoreCpuBandwidthStat> for CpuBandwidthStat {
  fn from(value: CoreCpuBandwidthStat) -> Self {
    Self {
      nr_periods: value.nr_periods().value,
      nr_throttled: value.nr_throttled().value,
      throttled_ns: value.throttled().get::<nanosecond>(),
      burst: value.burst().map(Into::into),
    }
  }
}

/// CPU bandwidth burst counters.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
struct CpuBurstStat {
  /// Number of bursts.
  nr_bursts: u64,
  /// Cumulative burst time in nanoseconds.
  burst_ns: u64,
}

impl From<CoreCpuBurstStat> for CpuBurstStat {
  fn from(value: CoreCpuBurstStat) -> Self {
    Self { nr_bursts: value.nr_bursts().value, burst_ns: value.burst().get::<nanosecond>() }
  }
}

/// This cgroup's CPU quota and period, independent of ancestor limits.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Clone)]
struct CpuMax {
  /// Quota in nanoseconds or the kernel's `max`.
  quota_ns: MaxOr,
  /// Bandwidth period in nanoseconds.
  period_ns: u64,
  /// Quota divided by period, or `max` at this cgroup only.
  cpu_count: MaxOr,
}

impl From<CoreCpuMax> for CpuMax {
  fn from(value: CoreCpuMax) -> Self {
    Self {
      quota_ns: MaxOr::from_time(value.quota()),
      period_ns: value.period().get::<nanosecond>(),
      cpu_count: MaxOr::from_ratio(value.cpu_count()),
    }
  }
}

/// CPU scheduling weight or the special idle state.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
struct CpuWeight {
  /// Whether this cgroup uses the special idle scheduling weight.
  is_idle: bool,
  /// Weight from 1 through 10,000; None in the idle state.
  shares: Option<u16>,
}

impl From<CoreCpuWeight> for CpuWeight {
  fn from(value: CoreCpuWeight) -> Self {
    match value {
      | CoreCpuWeight::Idle => Self { is_idle: true, shares: None },
      | CoreCpuWeight::Shares(weight) => Self { is_idle: false, shares: Some(*weight.as_ref()) },
    }
  }
}

/// Pressure-stall information with some and optional full lines.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
struct Pressure {
  /// Time during which some tasks were stalled.
  some: PressureLine,
  /// Time during which all tasks were stalled, if reported.
  full: Option<PressureLine>,
}

impl From<CorePressure> for Pressure {
  fn from(value: CorePressure) -> Self { Self { some: value.some().into(), full: value.full().map(Into::into) } }
}

/// Pressure averages as ratios and cumulative stall time in nanoseconds.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
struct PressureLine {
  /// Ten-second pressure average, from zero to one.
  avg10: f64,
  /// Sixty-second pressure average, from zero to one.
  avg60: f64,
  /// Three-hundred-second pressure average, from zero to one.
  avg300: f64,
  /// Cumulative stall time in nanoseconds.
  total_ns: u64,
}

impl From<CorePressureLine> for PressureLine {
  fn from(value: CorePressureLine) -> Self {
    Self {
      avg10: value.avg10().get::<ratio>(),
      avg60: value.avg60().get::<ratio>(),
      avg300: value.avg300().get::<ratio>(),
      total_ns: value.total().get::<nanosecond>(),
    }
  }
}

/// Fresh reads of memory controller files for one pinned cgroup.
#[pyclass(frozen, module = "sakai._sakai")]
struct MemoryReader {
  owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl MemoryReader {
  /// Return a swap reader that retains this pinned cgroup handle.
  fn swap(&self) -> SwapReader { SwapReader { owner: Arc::clone(&self.owner) } }

  /// Return a compressed swap reader that retains this pinned cgroup handle.
  fn zswap(&self) -> ZswapReader { ZswapReader { owner: Arc::clone(&self.owner) } }

  /// Read current memory usage in bytes.
  fn current(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().current()).map(|value| value.value().get::<byte>())
  }

  /// Read peak memory usage in bytes.
  fn peak(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().peak()).map(|value| value.value().get::<byte>())
  }

  /// Read the hard memory limit as `max` or a concrete byte count.
  fn max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().max()).map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read the throttling threshold as `max` or a concrete byte count.
  fn high(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().high()).map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read the best-effort memory protection boundary in bytes.
  fn low(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().low()).map(|value| value.value().get::<byte>())
  }

  /// Read the hard memory protection boundary in bytes.
  fn min(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().min()).map(|value| value.value().get::<byte>())
  }

  /// Read whether the OOM killer treats this cgroup as one workload.
  fn oom_group(&self, py: Python<'_>) -> PyResult<bool> {
    PythonError::read(py, &self.owner, |owner| owner.memory().oom_group()).map(|value| value.value())
  }

  /// Read hierarchical memory events; memory_localevents mounts report local events.
  fn events(&self, py: Python<'_>) -> PyResult<MemoryEventCounts> {
    PythonError::read(py, &self.owner, |owner| owner.memory().events()).map(|value| value.counts().into())
  }

  /// Read events originating in this cgroup, excluding descendants.
  /// Older kernels may lack this interface, raising InterfaceMissingError.
  fn events_local(&self, py: Python<'_>) -> PyResult<MemoryEventCounts> {
    PythonError::read(py, &self.owner, |owner| owner.memory().events_local()).map(|value| value.counts().into())
  }

  /// Read memory statistics in separate byte, page, and count mappings.
  fn stat(&self, py: Python<'_>) -> PyResult<MemoryStat> {
    PythonError::read(py, &self.owner, |owner| owner.memory().stat()).map(Into::into)
  }

  /// Read memory statistics by field and NUMA node, grouped by native units.
  fn numa_stat(&self, py: Python<'_>) -> PyResult<MemoryNumaStat> {
    PythonError::read(py, &self.owner, |owner| owner.memory().numa_stat()).map(Into::into)
  }

  /// Read memory pressure-stall averages and cumulative times.
  fn pressure(&self, py: Python<'_>) -> PyResult<Pressure> {
    PythonError::read(py, &self.owner, |owner| owner.memory().pressure()).map(Into::into)
  }
}

/// Immutable occurrence counts from memory.events or memory.events.local.
///
/// Missing optional counters are None, distinct from zero. Unknown kernel
/// fields are ignored. The reader method determines whether counts include descendants.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryEventCounts {
  /// Reclaims despite usage below the effective low boundary.
  low: u64,
  /// Throttling and direct reclaim after crossing the high boundary.
  high: u64,
  /// Attempts to cross the max boundary.
  max: u64,
  /// Allocations about to fail at the memory limit.
  oom: u64,
  /// Processes killed by any OOM killer.
  oom_kill: u64,
  /// Group OOM kills, or None when the kernel omits the counter.
  oom_group_kill: Option<u64>,
  /// Network socket throttling events, or None when the kernel omits the counter.
  sock_throttled: Option<u64>,
}

impl From<CoreMemoryEventCounts> for MemoryEventCounts {
  fn from(value: CoreMemoryEventCounts) -> Self {
    Self {
      low: value.low().value,
      high: value.high().value,
      max: value.max().value,
      oom: value.oom().value,
      oom_kill: value.oom_kill().value,
      oom_group_kill: value.oom_group_kill().map(|count| count.value),
      sock_throttled: value.sock_throttled().map(|count| count.value),
    }
  }
}

/// Fresh reads of swap usage, limits, and events for one pinned cgroup.
#[pyclass(frozen, module = "sakai._sakai")]
struct SwapReader {
  owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl SwapReader {
  /// Read current hierarchical swap usage in bytes.
  fn current(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().current()).map(|value| value.value().get::<byte>())
  }

  /// Read peak swap usage since cgroup creation; this never resets the peak.
  fn peak(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().peak()).map(|value| value.value().get::<byte>())
  }

  /// Read the hard swap limit as `max` or a concrete byte count.
  fn max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().max())
      .map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read the swap throttling threshold as `max` or a concrete byte count.
  fn high(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().high())
      .map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read swap threshold and allocation failure counters.
  fn events(&self, py: Python<'_>) -> PyResult<SwapEvents> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().events()).map(Into::into)
  }
}

/// Immutable swap threshold and allocation failure counters.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
struct SwapEvents {
  /// Times swap usage exceeded the high threshold; absent on older kernels.
  high: Option<u64>,
  /// Times swap allocation failed at the max boundary.
  max: u64,
  /// Swap allocation failures from the limit or system-wide exhaustion.
  fail: u64,
}

impl From<CoreSwapEvents> for SwapEvents {
  fn from(value: CoreSwapEvents) -> Self {
    Self { high: value.high().map(|count| count.value), max: value.max().value, fail: value.fail().value }
  }
}

/// Fresh reads of compressed swap usage and settings for one pinned cgroup.
#[pyclass(frozen, module = "sakai._sakai")]
struct ZswapReader {
  owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl ZswapReader {
  /// Read memory consumed by the zswap compression backend in bytes.
  fn current(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().zswap().current())
      .map(|value| value.value().get::<byte>())
  }

  /// Read the hard zswap pool limit as `max` or a concrete byte count.
  fn max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().zswap().max())
      .map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read the configured disk writeback policy; an ancestor can disable it.
  fn writeback(&self, py: Python<'_>) -> PyResult<bool> {
    PythonError::read(py, &self.owner, |owner| owner.memory().zswap().writeback()).map(|value| value.value())
  }
}

/// Memory statistics grouped by their native units.
///
/// Keys retain the kernel's snake_case names. Unknown kernel fields are
/// ignored; each exposed mapping is read-only.
#[pyclass(frozen, module = "sakai._sakai")]
struct MemoryStat {
  bytes: BTreeMap<&'static str, u64>,
  pages: BTreeMap<&'static str, u64>,
  counts: BTreeMap<&'static str, u64>,
}

impl From<CoreMemoryStat> for MemoryStat {
  fn from(value: CoreMemoryStat) -> Self {
    Self {
      bytes: MemoryValues::fields(value.bytes(), |value| value.get::<byte>()),
      pages: MemoryValues::fields(value.pages(), |value| value.value),
      counts: MemoryValues::fields(value.counts(), |value| value.value),
    }
  }
}

#[pymethods]
impl MemoryStat {
  /// Read-only mapping of memory.stat byte-valued fields.
  #[getter]
  fn bytes(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.bytes) }

  /// Read-only mapping of memory.stat page-valued fields.
  #[getter]
  fn pages(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.pages) }

  /// Read-only mapping of memory.stat count-valued fields.
  #[getter]
  fn counts(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.counts) }
}

impl MemoryStat {
  fn mapping(py: Python<'_>, values: &BTreeMap<&str, u64>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, values.into_py_dict(py)?.as_mapping()).unbind())
  }
}

type NumaValues = BTreeMap<&'static str, BTreeMap<u32, u64>>;

/// Converts memory field names and quantities without assuming a page size.
struct MemoryValues;

impl MemoryValues {
  fn fields<Field: Copy + Into<&'static str>, Value, Output>(
    values: &BTreeMap<Field, Value>,
    convert: impl Fn(&Value) -> Output,
  ) -> BTreeMap<&'static str, Output> {
    values.iter().map(|(key, value)| ((*key).into(), convert(value))).collect()
  }

  fn nodes<Value>(values: &BTreeMap<u32, Value>, convert: impl Fn(&Value) -> u64) -> BTreeMap<u32, u64> {
    values.iter().map(|(node, value)| (*node, convert(value))).collect()
  }
}

/// Immutable per-NUMA-node memory statistics grouped by native units.
///
/// Outer keys retain kernel field names; inner keys are integer NUMA node IDs.
/// Both levels of every mapping are read-only. Unknown fields are ignored.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
struct MemoryNumaStat {
  bytes: NumaValues,
  pages: NumaValues,
  counts: NumaValues,
}

impl From<CoreMemoryNumaStat> for MemoryNumaStat {
  fn from(value: CoreMemoryNumaStat) -> Self {
    Self {
      bytes: MemoryValues::fields(value.bytes(), |nodes| MemoryValues::nodes(nodes, |value| value.get::<byte>())),
      pages: MemoryValues::fields(value.pages(), |nodes| MemoryValues::nodes(nodes, |value| value.value)),
      counts: MemoryValues::fields(value.counts(), |nodes| MemoryValues::nodes(nodes, |value| value.value)),
    }
  }
}

#[pymethods]
impl MemoryNumaStat {
  /// Read-only byte amounts by memory field and NUMA node ID.
  #[getter]
  fn bytes(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.bytes) }

  /// Read-only page quantities by memory field and NUMA node ID.
  #[getter]
  fn pages(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.pages) }

  /// Read-only event counts by memory field and NUMA node ID.
  #[getter]
  fn counts(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.counts) }
}

impl MemoryNumaStat {
  fn mapping(py: Python<'_>, values: &NumaValues) -> PyResult<Py<PyMappingProxy>> {
    let dict = PyDict::new(py);
    for (key, nodes) in values {
      dict.set_item(key, PyMappingProxy::new(py, nodes.into_py_dict(py)?.as_mapping()))?;
    }
    Ok(PyMappingProxy::new(py, dict.as_mapping()).unbind())
  }
}

/// Fresh process-number reads for one pinned cgroup. Counts include threads.
///
/// Separate calls are not atomic together. Missing files raise InterfaceMissingError.
#[pyclass(frozen, module = "sakai._sakai")]
struct PidsReader {
  owner: Arc<sakai::Cgroup>,
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
struct PidsEvents {
  /// Number of occurrences of the process limit being hit.
  max: u64,
}

impl From<CorePidsEvents> for PidsEvents {
  fn from(value: CorePidsEvents) -> Self { Self { max: value.max().value } }
}

/// Immutable lifecycle snapshot; states can change immediately after a read.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct CgroupEvents {
  /// Whether this cgroup or any descendant contains live processes.
  populated: bool,
  /// Completed freezing, including ancestors; None when an older kernel omits it.
  frozen: Option<bool>,
}

impl From<CoreCgroupEvents> for CgroupEvents {
  fn from(value: CoreCgroupEvents) -> Self { Self { populated: value.populated(), frozen: value.frozen() } }
}

/// Immutable, volatile cgroup and subsystem object counts from cgroup.stat.
///
/// Subsystem maps use controller names, including unknown names. Missing
/// counters are absent from the maps, rather than zero. All mappings are read-only.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
struct CgroupStat {
  descendants: u64,
  dying_descendants: u64,
  subsystems: BTreeMap<String, u64>,
  dying_subsystems: BTreeMap<String, u64>,
}

impl From<CoreCgroupStat> for CgroupStat {
  fn from(value: CoreCgroupStat) -> Self {
    Self {
      descendants: value.descendants().value,
      dying_descendants: value.dying_descendants().value,
      subsystems: value.subsystems().iter().map(|(controller, count)| (controller.to_string(), count.value)).collect(),
      dying_subsystems: value
        .dying_subsystems()
        .iter()
        .map(|(controller, count)| (controller.to_string(), count.value))
        .collect(),
    }
  }
}

#[pymethods]
impl CgroupStat {
  /// Visible descendant cgroup count, excluding this cgroup.
  #[getter]
  fn descendants(&self) -> u64 { self.descendants }

  /// Deleted descendant cgroups awaiting final destruction.
  #[getter]
  fn dying_descendants(&self) -> u64 { self.dying_descendants }

  /// Read-only live subsystem counts at and beneath this cgroup.
  #[getter]
  fn subsystems(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, self.subsystems.clone().into_py_dict(py)?.as_mapping()).unbind())
  }

  /// Read-only dying subsystem counts at and beneath this cgroup.
  #[getter]
  fn dying_subsystems(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, self.dying_subsystems.clone().into_py_dict(py)?.as_mapping()).unbind())
  }
}

/// Fresh reads of cgroup topology, controller, and lifecycle files.
#[pyclass(frozen, module = "sakai._sakai")]
struct CoreReader {
  owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl CoreReader {
  /// Read volatile populated and frozen state; root cgroups lack this interface.
  fn events(&self, py: Python<'_>) -> PyResult<CgroupEvents> {
    PythonError::read(py, &self.owner, |owner| owner.core().events()).map(Into::into)
  }

  /// Read volatile descendant and subsystem counts, including at the hierarchy root.
  fn stat(&self, py: Python<'_>) -> PyResult<CgroupStat> {
    PythonError::read(py, &self.owner, |owner| owner.core().stat()).map(Into::into)
  }

  /// Read the cgroup type: domain, domain threaded, domain invalid, or threaded.
  fn kind(&self, py: Python<'_>) -> PyResult<String> {
    PythonError::read(py, &self.owner, |owner| owner.core().kind()).map(|kind| kind.to_string())
  }

  /// List controllers available to this cgroup, including unknown names.
  fn controllers(&self, py: Python<'_>) -> PyResult<Vec<String>> {
    PythonError::read(py, &self.owner, |owner| owner.core().controllers())
      .map(|values| values.into_iter().map(|value| value.to_string()).collect())
  }

  /// List controllers enabled for children of this cgroup.
  fn subtree_control(&self, py: Python<'_>) -> PyResult<Vec<String>> {
    PythonError::read(py, &self.owner, |owner| owner.core().subtree_control())
      .map(|values| values.into_iter().map(|value| value.to_string()).collect())
  }
}

/// Read-only Linux cgroup v2 bindings backed by sakai.
#[pymodule]
fn _sakai(module: &Bound<'_, PyModule>) -> PyResult<()> {
  module.add("SakaiError", module.py().get_type::<SakaiError>())?;
  module.add("InterfaceMissingError", module.py().get_type::<InterfaceMissingError>())?;
  module.add("NotCgroupV2Error", module.py().get_type::<NotCgroupV2Error>())?;
  module.add("DeletedCgroupError", module.py().get_type::<DeletedCgroupError>())?;
  module.add("CgroupParseError", module.py().get_type::<CgroupParseError>())?;
  module.add("NotSupportedError", module.py().get_type::<NotSupportedError>())?;
  module.add_class::<Cgroup>()?;
  module.add_class::<MaxOr>()?;
  module.add_class::<CpuReader>()?;
  module.add_class::<MemoryReader>()?;
  module.add_class::<SwapReader>()?;
  module.add_class::<ZswapReader>()?;
  module.add_class::<CoreReader>()?;
  module.add_class::<PidsReader>()?;
  module.add_class::<PidsEvents>()?;
  module.add_class::<CgroupEvents>()?;
  module.add_class::<CgroupStat>()?;
  module.add_class::<CpuStat>()?;
  module.add_class::<CpuStatLocal>()?;
  module.add_class::<CpuTimeStat>()?;
  module.add_class::<CpuBandwidthStat>()?;
  module.add_class::<CpuBurstStat>()?;
  module.add_class::<CpuMax>()?;
  module.add_class::<CpuWeight>()?;
  module.add_class::<Pressure>()?;
  module.add_class::<PressureLine>()?;
  module.add_class::<MemoryStat>()?;
  module.add_class::<MemoryEventCounts>()?;
  module.add_class::<SwapEvents>()?;
  module.add_class::<MemoryNumaStat>()?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{
    ffi::OsString,
    io,
    os::unix::ffi::{OsStrExt, OsStringExt},
  };

  use assertables::{assert_err, assert_in_delta, assert_ok};
  use pyo3::{
    exceptions::{PyAttributeError, PyFileNotFoundError, PyPermissionError, PyTypeError},
    types::{PyBytes, PyFloat, PyInt},
  };

  use super::*;

  #[test]
  fn converts_cpu_stat_optional_fields_and_nanoseconds() {
    let time = CpuTimeStat { usage_ns: 10_000, user_ns: 7_000, system_ns: 3_000 };
    for (input, bandwidth) in [
      ("usage_usec 10\nuser_usec 7\nsystem_usec 3\n", None),
      (
        "usage_usec 10\nuser_usec 7\nsystem_usec 3\nnr_periods 5\nnr_throttled 2\nthrottled_usec 4\nnr_bursts \
         1\nburst_usec 6\n",
        Some(CpuBandwidthStat {
          nr_periods: 5,
          nr_throttled: 2,
          throttled_ns: 4_000,
          burst: Some(CpuBurstStat { nr_bursts: 1, burst_ns: 6_000 }),
        }),
      ),
    ] {
      assert_eq!(CpuStat::from(assert_ok!(input.parse::<CoreCpuStat>())), CpuStat { time: time.clone(), bandwidth });
    }
    for (input, throttled_ns) in [("", None), ("throttled_usec 0", Some(0))] {
      assert_eq!(CpuStatLocal::from(assert_ok!(input.parse::<CoreCpuStatLocal>())), CpuStatLocal { throttled_ns });
    }
  }

  #[test]
  fn converts_limits_and_idle_without_collapsing_zero() {
    let unlimited = CpuMax::from(assert_ok!("max 100000".parse::<CoreCpuMax>()));
    let limited = CpuMax::from(assert_ok!("25000 100000".parse::<CoreCpuMax>()));
    assert!(unlimited.quota_ns.is_max());
    assert_err!(unlimited.quota_ns.value());
    assert!(unlimited.cpu_count.is_max());
    assert_err!(unlimited.cpu_count.value());
    assert!(matches!(assert_ok!(limited.quota_ns.value()), PythonLimitValue::Integer(25_000_000)));
    assert_eq!(limited.period_ns, 100_000_000);
    assert!(matches!(assert_ok!(limited.cpu_count.value()), PythonLimitValue::Ratio(0.25)));

    let idle = CpuWeight::from(assert_ok!("0".parse::<CoreCpuWeight>()));
    let weighted = CpuWeight::from(assert_ok!("100".parse::<CoreCpuWeight>()));
    assert!(idle.is_idle);
    assert_eq!(idle.shares, None);
    assert!(!weighted.is_idle);
    assert_eq!(weighted.shares, Some(100));

    let max = MaxOr::from_bytes(CoreMaxOr::Max);
    assert!(max.is_max());
    assert_err!(max.value());
    assert!(matches!(
      assert_ok!(MaxOr::from_bytes(CoreMaxOr::Value(Bytes::new::<byte>(0))).value()),
      PythonLimitValue::Integer(0)
    ));
  }

  #[test]
  fn max_or_exposes_typed_python_values_and_guards_max() {
    Python::initialize();
    Python::attach(|py| {
      let integer = assert_ok!(Py::new(py, MaxOr::from_bytes(CoreMaxOr::Value(Bytes::new::<byte>(0)))));
      let integer_value = assert_ok!(integer.bind(py).getattr("value"));
      assert!(integer_value.is_instance_of::<PyInt>());
      assert_eq!(assert_ok!(integer_value.extract::<u64>()), 0);

      let ratio_limit = assert_ok!(Py::new(py, MaxOr::from_ratio(CoreMaxOr::Value(Ratio::new::<ratio>(0.25)))));
      let ratio_value = assert_ok!(ratio_limit.bind(py).getattr("value"));
      assert!(ratio_value.is_instance_of::<PyFloat>());
      assert_in_delta!(assert_ok!(ratio_value.extract::<f64>()), 0.25, f64::EPSILON);

      let max = assert_ok!(Py::new(py, MaxOr::from_bytes(CoreMaxOr::Max)));
      let error = assert_err!(max.bind(py).getattr("value"));
      assert!(error.is_instance_of::<PyValueError>(py));
    });
  }

  #[test]
  fn converts_pressure_percent_and_optional_full_line() {
    let pressure =
      Pressure::from(assert_ok!("some avg10=12.50 avg60=0.00 avg300=100.00 total=23\n".parse::<CorePressure>()));
    assert_in_delta!(pressure.some.avg10, 0.125, f64::EPSILON);
    assert_eq!(pressure.some.total_ns, 23_000);
    assert!(pressure.full.is_none());
  }

  #[test]
  fn separates_memory_bytes_pages_and_counts() {
    let stat = MemoryStat::from(assert_ok!("anon 4096\nfile 2048\npswpin 3\npgfault 7\n".parse::<CoreMemoryStat>()));
    assert_eq!(stat.bytes.get("anon"), Some(&4096));
    assert_eq!(stat.pages.get("pswpin"), Some(&3));
    assert_eq!(stat.counts.get("pgfault"), Some(&7));
    assert!(!stat.bytes.contains_key("pswpin"));
    assert!(!stat.pages.contains_key("pgfault"));
  }

  #[test]
  fn preserves_memory_optional_events_zero_and_unsigned_counts() {
    Python::initialize();
    Python::attach(|py| {
      for (input, expected) in [
        ("low 1\nhigh 2\nmax 3\noom 4\noom_kill 5", MemoryEventCounts {
          low: 1,
          high: 2,
          max: 3,
          oom: 4,
          oom_kill: 5,
          oom_group_kill: None,
          sock_throttled: None,
        }),
        (
          "low 0\nhigh 0\nmax 0\noom 0\noom_kill 18446744073709551615\noom_group_kill 0\nsock_throttled 0\nfuture nope",
          MemoryEventCounts {
            low: 0,
            high: 0,
            max: 0,
            oom: 0,
            oom_kill: u64::MAX,
            oom_group_kill: Some(0),
            sock_throttled: Some(0),
          },
        ),
      ] {
        for counts in [
          assert_ok!(input.parse::<sakai::v2::memory::MemoryEvents>()).counts(),
          assert_ok!(input.parse::<sakai::v2::memory::MemoryEventsLocal>()).counts(),
        ] {
          let events = MemoryEventCounts::from(counts);
          assert_eq!(events, expected);
          let object = assert_ok!(Py::new(py, events));
          assert_eq!(assert_ok!(assert_ok!(object.bind(py).getattr("oom_kill")).extract::<u64>()), expected.oom_kill);
          for (name, expected) in
            [("oom_group_kill", expected.oom_group_kill), ("sock_throttled", expected.sock_throttled)]
          {
            assert_eq!(assert_ok!(assert_ok!(object.bind(py).getattr(name)).extract::<Option<u64>>()), expected);
          }
          assert!(assert_err!(object.bind(py).setattr("low", 0)).is_instance_of::<PyAttributeError>(py));
        }
      }
    });
  }

  #[test]
  fn preserves_swap_optional_high_and_unsigned_counts() {
    Python::initialize();
    Python::attach(|py| {
      for (input, expected) in [
        ("max 1\nfail 2\n", SwapEvents { high: None, max: 1, fail: 2 }),
        ("high 0\nmax 0\nfail 18446744073709551615\n", SwapEvents { high: Some(0), max: 0, fail: u64::MAX }),
      ] {
        let events = SwapEvents::from(assert_ok!(input.parse::<CoreSwapEvents>()));
        assert_eq!(events, expected);
        let object = assert_ok!(Py::new(py, events));
        let high = assert_ok!(object.bind(py).getattr("high"));
        assert_eq!(assert_ok!(high.extract::<Option<u64>>()), expected.high);
        let error = assert_err!(object.bind(py).setattr("fail", 0));
        assert!(error.is_instance_of::<PyAttributeError>(py));
      }
    });
  }

  #[test]
  fn converts_numa_units_and_freezes_both_mapping_levels() {
    let stat = MemoryNumaStat::from(assert_ok!(
      "anon N0=4096 N2=8192\nfile N0=0\npgdemote_direct N2=3\nworkingset_refault_file N2=7\nfuture N0=nope\n"
        .parse::<CoreMemoryNumaStat>()
    ));
    assert_eq!(stat, MemoryNumaStat {
      bytes: BTreeMap::from([("anon", BTreeMap::from([(0, 4096), (2, 8192)])), ("file", BTreeMap::from([(0, 0)])),]),
      pages: BTreeMap::from([("pgdemote_direct", BTreeMap::from([(2, 3)]))]),
      counts: BTreeMap::from([("workingset_refault_file", BTreeMap::from([(2, 7)]))]),
    });

    Python::initialize();
    Python::attach(|py| {
      let object = assert_ok!(Py::new(py, stat));
      for (name, field, expected) in [
        ("bytes", "anon", BTreeMap::from([(0_u32, 4096_u64), (2, 8192)])),
        ("pages", "pgdemote_direct", BTreeMap::from([(2, 3)])),
        ("counts", "workingset_refault_file", BTreeMap::from([(2, 7)])),
      ] {
        let mapping = assert_ok!(object.bind(py).getattr(name));
        let nodes = assert_ok!(mapping.get_item(field));
        let values = assert_ok!(py.get_type::<PyDict>().call1((&nodes,)));
        assert_eq!(assert_ok!(values.extract::<BTreeMap<u32, u64>>()), expected);
        assert!(assert_err!(mapping.set_item("future", 0)).is_instance_of::<PyTypeError>(py));
        assert!(assert_err!(nodes.set_item(0, 0)).is_instance_of::<PyTypeError>(py));
      }
      assert!(assert_err!(object.bind(py).setattr("bytes", 0)).is_instance_of::<PyAttributeError>(py));
    });
  }

  #[test]
  fn preserves_process_limits_states_and_immutable_subsystem_counts() {
    Python::initialize();
    Python::attach(|py| {
      for (input, expected) in [("0", Some(0)), ("18446744073709551615", Some(u64::MAX)), ("max", None)] {
        let limit = MaxOr::from_count(assert_ok!(input.parse::<sakai::v2::pids::PidsMax>()).value());
        assert_eq!(limit.is_max(), expected.is_none());
        match expected {
          | Some(expected) => {
            let object = assert_ok!(Py::new(py, limit));
            let value = assert_ok!(object.bind(py).getattr("value"));
            assert!(value.is_instance_of::<PyInt>());
            assert_eq!(assert_ok!(value.extract::<u64>()), expected);
          },
          | None => {
            assert_err!(limit.value());
          },
        }
      }
      for (input, expected) in [
        ("populated 1", CgroupEvents { populated: true, frozen: None }),
        ("frozen 0\npopulated 1", CgroupEvents { populated: true, frozen: Some(false) }),
        ("populated 0\nfrozen 1", CgroupEvents { populated: false, frozen: Some(true) }),
      ] {
        let state = CgroupEvents::from(assert_ok!(input.parse::<CoreCgroupEvents>()));
        assert_eq!(state, expected);
        let object = assert_ok!(Py::new(py, state));
        assert_eq!(
          assert_ok!(assert_ok!(object.bind(py).getattr("frozen")).extract::<Option<bool>>()),
          expected.frozen
        );
        assert!(assert_err!(object.bind(py).setattr("populated", false)).is_instance_of::<PyAttributeError>(py));
      }
      let events = PidsEvents::from(assert_ok!("max 18446744073709551615".parse::<CorePidsEvents>()));
      assert_eq!(events, PidsEvents { max: u64::MAX });
      let object = assert_ok!(Py::new(py, events));
      assert_eq!(assert_ok!(assert_ok!(object.bind(py).getattr("max")).extract::<u64>()), u64::MAX);
      assert!(assert_err!(object.bind(py).setattr("max", 0)).is_instance_of::<PyAttributeError>(py));
      for (input, expected) in [
        (
          "nr_descendants 2\nnr_dying_descendants 1\nnr_subsys_memory 3\nnr_dying_subsys_future_controller 0",
          CgroupStat {
            descendants: 2,
            dying_descendants: 1,
            subsystems: BTreeMap::from([("memory".into(), 3)]),
            dying_subsystems: BTreeMap::from([("future_controller".into(), 0)]),
          },
        ),
        ("nr_descendants 0\nnr_dying_descendants 0", CgroupStat {
          descendants: 0,
          dying_descendants: 0,
          subsystems: BTreeMap::new(),
          dying_subsystems: BTreeMap::new(),
        }),
      ] {
        let stat = CgroupStat::from(assert_ok!(input.parse::<CoreCgroupStat>()));
        assert_eq!(stat, expected);
        let object = assert_ok!(Py::new(py, stat));
        for (name, expected) in [("subsystems", expected.subsystems), ("dying_subsystems", expected.dying_subsystems)] {
          let mapping = assert_ok!(object.bind(py).getattr(name));
          let values = assert_ok!(py.get_type::<PyDict>().call1((&mapping,)));
          assert_eq!(assert_ok!(values.extract::<BTreeMap<String, u64>>()), expected);
          assert!(assert_err!(mapping.set_item("memory", 0)).is_instance_of::<PyTypeError>(py));
        }
        assert!(assert_err!(object.bind(py).setattr("descendants", 0)).is_instance_of::<PyAttributeError>(py));
      }
    });
  }

  #[test]
  fn preserves_python_error_categories_paths_and_errno() {
    Python::initialize();
    Python::attach(|py| {
      let path = PathBuf::from("/sys/fs/cgroup/example");
      for (error, exception_type, expected_path) in [
        (Error::FileMissing { path: path.clone() }, py.get_type::<InterfaceMissingError>(), Some(&path)),
        (Error::DeletedCgroup { path: path.clone() }, py.get_type::<DeletedCgroupError>(), Some(&path)),
        (
          Error::Parse { path: path.clone(), source: Box::new(io::Error::other("invalid quota")) },
          py.get_type::<CgroupParseError>(),
          Some(&path),
        ),
        (Error::NotCgroupV2, py.get_type::<NotCgroupV2Error>(), None),
        (Error::NotSupported, py.get_type::<NotSupportedError>(), None),
      ] {
        let translated = assert_err!(PythonError::result::<()>(py, Err(error)));
        assert!(translated.is_instance(py, &exception_type));
        assert!(translated.is_instance_of::<SakaiError>(py));
        assert!(!translated.is_instance_of::<PyOSError>(py));
        match expected_path {
          | Some(expected) => {
            let actual = assert_ok!(assert_ok!(translated.value(py).getattr("path")).extract::<PathBuf>());
            assert_eq!(&actual, expected);
          },
          | None => assert!(!assert_ok!(translated.value(py).hasattr("path"))),
        }
        if translated.is_instance_of::<CgroupParseError>(py) {
          assert!(translated.to_string().contains("invalid quota"));
        }
      }
      for (errno, exception_type) in
        [(2, py.get_type::<PyFileNotFoundError>()), (13, py.get_type::<PyPermissionError>())]
      {
        let translated = PythonError::from_core(py, Error::Io(io::Error::from_raw_os_error(errno)));
        assert!(translated.is_instance(py, &exception_type));
        assert!(!translated.is_instance_of::<SakaiError>(py));
        assert_eq!(assert_ok!(assert_ok!(translated.value(py).getattr("errno")).extract::<i32>()), errno);
      }
    });
  }

  #[test]
  fn preserves_non_utf8_path_bytes_in_both_directions() {
    Python::initialize();
    Python::attach(|py| {
      let raw = b"child-\xff";
      let name = PyBytes::new(py, raw);
      let extracted = assert_ok!(PythonPath::extract(&name));
      assert_eq!(extracted.as_os_str().as_bytes(), raw);
      let decoded = assert_ok!(assert_ok!(py.import("os")).call_method1("fsdecode", (name,)));
      let extracted = assert_ok!(PythonPath::extract(&decoded));
      assert_eq!(extracted.as_os_str().as_bytes(), raw);
      let pathlike =
        assert_ok!(py.eval(c"type('BytePath', (), {'__fspath__': lambda self: b'child-\\xff'})()", None, None,));
      let extracted = assert_ok!(PythonPath::extract(&pathlike));
      assert_eq!(extracted.as_os_str().as_bytes(), raw);

      let error = PythonError::from_core(py, Error::FileMissing {
        path: PathBuf::from(OsString::from_vec(b"/sys/fs/cgroup/child-\xff/cpu.stat.local".to_vec())),
      });
      let path = assert_ok!(error.value(py).getattr("path"));
      let encoded = assert_ok!(assert_ok!(py.import("os")).call_method1("fsencode", (path,)));
      assert_eq!(assert_ok!(encoded.cast::<PyBytes>()).as_bytes(), b"/sys/fs/cgroup/child-\xff/cpu.stat.local");
    });
  }
}
