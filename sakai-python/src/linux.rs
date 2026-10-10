//! Linux cgroup v2 Python bindings.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use pyo3::{
  PyTypeInfo, create_exception,
  exceptions::{PyException, PyOSError, PyValueError},
  prelude::*,
  types::{PyDict, PyMappingProxy, PyModule},
};
use sakai_core::{
  Bytes, Error, MaxOr as CoreMaxOr, NonZeroTime, Pressure as CorePressure,
  PressureLine as CorePressureLine, Ratio,
  v2::{
    cpu::{
      CpuBandwidthStat as CoreCpuBandwidthStat,
      CpuBurstStat as CoreCpuBurstStat, CpuMax as CoreCpuMax,
      CpuStat as CoreCpuStat, CpuStatLocal as CoreCpuStatLocal,
      CpuTimeStat as CoreCpuTimeStat, CpuWeight as CoreCpuWeight,
    },
    memory::{
      MemoryNumaStat as CoreMemoryNumaStat, MemoryStat as CoreMemoryStat,
      SwapEvents as CoreSwapEvents,
    },
  },
};
use uom::si::{information::byte, ratio::ratio, time::nanosecond};

create_exception!(
  _sakai,
  SakaiError,
  PyException,
  "Base error for Sakai cgroup operations."
);
create_exception!(
  _sakai,
  InterfaceMissingError,
  SakaiError,
  "A cgroup interface file is missing; path identifies the interface."
);
create_exception!(
  _sakai,
  NotCgroupV2Error,
  SakaiError,
  "The requested path is not on a cgroup v2 filesystem."
);
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
create_exception!(
  _sakai,
  NotSupportedError,
  SakaiError,
  "The operation is not supported for this cgroup."
);

struct PythonError;

struct PythonPath;

impl PythonPath {
  fn extract<T>(object: &Bound<'_, T>) -> PyResult<PathBuf> {
    // fsdecode first applies os.fspath and decodes bytes with surrogateescape.
    // PyO3 0.29's PathBuf extractor accepts only str results from os.fspath.
    object
      .py()
      .import("os")?
      .call_method1("fsdecode", (object.as_any(),))?
      .extract()
  }
}

impl PythonError {
  fn with_path<E: PyTypeInfo>(
    py: Python<'_>,
    message: String,
    path: PathBuf,
  ) -> PyErr {
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
        Self::with_path::<InterfaceMissingError>(
          py,
          format!("cgroup interface is missing: {}", path.display()),
          path,
        )
      },
      | Error::NotSupported => {
        NotSupportedError::new_err("operation is not supported for this cgroup")
      },
      | Error::NotCgroupV2 => {
        NotCgroupV2Error::new_err("not a cgroup v2 filesystem")
      },
      | Error::DeletedCgroup { path } => Self::with_path::<DeletedCgroupError>(
        py,
        format!("cgroup has been deleted: {}", path.display()),
        path,
      ),
      | Error::Parse { path, source } => Self::with_path::<CgroupParseError>(
        py,
        format!("failed to parse {}: {source}", path.display()),
        path,
      ),
      | Error::Io(source) => match source.raw_os_error() {
        | Some(errno) => PyOSError::new_err((errno, source.to_string())),
        | None => PyOSError::new_err(source.to_string()),
      },
    }
  }

  fn result<T>(py: Python<'_>, result: Result<T, Error>) -> PyResult<T> {
    result.map_err(|error| Self::from_core(py, error))
  }

  fn read<T: Send + 'static>(
    py: Python<'_>,
    owner: &Arc<sakai_core::Cgroup>,
    read: impl FnOnce(&sakai_core::Cgroup) -> Result<T, Error> + Send + 'static,
  ) -> PyResult<T> {
    let owner = Arc::clone(owner);
    Self::result(py, py.detach(move || read(&owner)))
  }
}

/// An open, pinned cgroup v2 directory handle.
///
/// Readers borrow this handle's ownership, so they remain usable after the
/// Python Cgroup object is released. Each read fetches a fresh kernel value;
/// separate calls do not form an atomic snapshot.
#[pyclass(frozen, module = "sakai._sakai")]
struct Cgroup {
  owner: Arc<sakai_core::Cgroup>,
}

impl From<sakai_core::Cgroup> for Cgroup {
  fn from(owner: sakai_core::Cgroup) -> Self {
    Self {
      owner: Arc::new(owner),
    }
  }
}

#[pymethods]
impl Cgroup {
  /// Open the cgroup containing the current process.
  #[staticmethod]
  fn current(py: Python<'_>) -> PyResult<Self> {
    PythonError::result(py, py.detach(sakai_core::Cgroup::from_current_process))
      .map(Into::into)
  }

  /// Open the cgroup containing a process ID.
  #[staticmethod]
  fn from_pid(py: Python<'_>, pid: i64) -> PyResult<Self> {
    let pid = u32::try_from(pid)?;
    PythonError::result(
      py,
      py.detach(move || sakai_core::Cgroup::from_pid(pid)),
    )
    .map(Into::into)
  }

  /// Open a cgroup v2 directory by its filesystem path.
  ///
  /// Accepts str, bytes, and os.PathLike without lossy path conversion.
  #[staticmethod]
  fn from_path(
    py: Python<'_>,
    #[pyo3(from_py_with = PythonPath::extract)] path: PathBuf,
  ) -> PyResult<Self> {
    PythonError::result(
      py,
      py.detach(move || sakai_core::Cgroup::from_path(&path)),
    )
    .map(Into::into)
  }

  /// Diagnostic filesystem path; it may become stale after a rename.
  #[getter]
  fn path(&self) -> PathBuf {
    self.owner.path().to_owned()
  }

  /// Open a direct child by one filesystem name, without following slashes.
  fn child(
    &self,
    py: Python<'_>,
    #[pyo3(from_py_with = PythonPath::extract)] name: PathBuf,
  ) -> PyResult<Self> {
    PythonError::read(py, &self.owner, move |owner| {
      owner.child(name.as_os_str())
    })
    .map(Into::into)
  }

  /// Return separate pinned handles for the current direct children.
  fn children(&self, py: Python<'_>) -> PyResult<Vec<Self>> {
    PythonError::read(py, &self.owner, sakai_core::Cgroup::children)
      .map(|children| children.into_iter().map(Into::into).collect())
  }

  /// Return a reader for CPU controller interfaces.
  fn cpu(&self) -> CpuReader {
    CpuReader {
      owner: Arc::clone(&self.owner),
    }
  }

  /// Return a reader for memory controller interfaces.
  fn memory(&self) -> MemoryReader {
    MemoryReader {
      owner: Arc::clone(&self.owner),
    }
  }

  /// Return a reader for core cgroup topology interfaces.
  fn core(&self) -> CoreReader {
    CoreReader {
      owner: Arc::clone(&self.owner),
    }
  }

  fn __repr__(&self) -> String {
    format!("Cgroup({:?})", self.owner.path())
  }
}

#[derive(Clone, Copy)]
enum LimitValue {
  Max,
  Integer(u64),
  Ratio(f64),
}

#[derive(Debug, pyo3::IntoPyObject)]
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
  inner: LimitValue,
}

impl MaxOr {
  fn from_time(value: CoreMaxOr<NonZeroTime>) -> Self {
    Self {
      inner: match value {
        | CoreMaxOr::Max => LimitValue::Max,
        | CoreMaxOr::Value(time) => {
          LimitValue::Integer(time.get::<nanosecond>())
        },
      },
    }
  }

  fn from_bytes(value: CoreMaxOr<Bytes>) -> Self {
    Self {
      inner: match value {
        | CoreMaxOr::Max => LimitValue::Max,
        | CoreMaxOr::Value(bytes) => LimitValue::Integer(bytes.get::<byte>()),
      },
    }
  }

  fn from_ratio(value: CoreMaxOr<Ratio>) -> Self {
    Self {
      inner: match value {
        | CoreMaxOr::Max => LimitValue::Max,
        | CoreMaxOr::Value(value) => LimitValue::Ratio(value.get::<ratio>()),
      },
    }
  }
}

#[pymethods]
impl MaxOr {
  /// Whether the kernel reported `max`.
  #[getter]
  fn is_max(&self) -> bool {
    matches!(self.inner, LimitValue::Max)
  }

  /// Concrete value; raises ValueError when the kernel reported `max`.
  #[getter]
  fn value(&self) -> PyResult<PythonLimitValue> {
    match self.inner {
      | LimitValue::Max => {
        Err(PyValueError::new_err("max has no numeric value"))
      },
      | LimitValue::Integer(value) => Ok(PythonLimitValue::Integer(value)),
      | LimitValue::Ratio(value) => Ok(PythonLimitValue::Ratio(value)),
    }
  }

  fn __repr__(&self) -> String {
    match self.inner {
      | LimitValue::Max => "MaxOr(max)".into(),
      | LimitValue::Integer(value) => format!("MaxOr(value={value})"),
      | LimitValue::Ratio(value) => format!("MaxOr(value={value})"),
    }
  }
}

/// Fresh reads of CPU controller files for one pinned cgroup.
#[pyclass(frozen, module = "sakai._sakai")]
struct CpuReader {
  owner: Arc<sakai_core::Cgroup>,
}

#[pymethods]
impl CpuReader {
  /// Read hierarchical CPU usage and optional bandwidth counters.
  fn stat(&self, py: Python<'_>) -> PyResult<CpuStat> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().stat())
      .map(|inner| CpuStat { inner })
  }

  /// Read local CPU throttling counters, if the interface exists.
  fn stat_local(&self, py: Python<'_>) -> PyResult<CpuStatLocal> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().stat_local())
      .map(|inner| CpuStatLocal { inner })
  }

  /// Read this cgroup's quota and period, not effective ancestor capacity.
  fn max(&self, py: Python<'_>) -> PyResult<CpuMax> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().max())
      .map(Into::into)
  }

  /// Read CPU weight, preserving the special idle state.
  fn weight(&self, py: Python<'_>) -> PyResult<CpuWeight> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().weight())
      .map(Into::into)
  }

  /// Read CPU weight as a nice value from -20 through 19.
  fn weight_nice(&self, py: Python<'_>) -> PyResult<i8> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().weight_nice())
      .map(|nice| *nice.as_ref())
  }

  /// Read the allowed CPU bandwidth burst in nanoseconds.
  fn max_burst(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().max_burst())
      .map(|burst| burst.value().get::<nanosecond>())
  }

  /// Read whether this cgroup is configured for idle CPU scheduling.
  fn idle(&self, py: Python<'_>) -> PyResult<bool> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().idle())
      .map(|idle| idle.value())
  }

  /// Read the minimum utilization clamp as a ratio from zero to one.
  fn uclamp_min(&self, py: Python<'_>) -> PyResult<f64> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().uclamp_min())
      .map(|clamp| clamp.value().get::<ratio>())
  }

  /// Read the maximum utilization clamp as `max` or a concrete ratio.
  fn uclamp_max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().uclamp_max())
      .map(|clamp| MaxOr::from_ratio(clamp.value()))
  }

  /// Read CPU pressure-stall averages and cumulative times.
  fn pressure(&self, py: Python<'_>) -> PyResult<Pressure> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().pressure())
      .map(Into::into)
  }
}

/// Hierarchical CPU usage and optional bandwidth statistics.
#[pyclass(frozen, module = "sakai._sakai")]
struct CpuStat {
  inner: CoreCpuStat,
}

#[pymethods]
impl CpuStat {
  /// CPU usage split into total, user, and system nanoseconds.
  #[getter]
  fn time(&self) -> CpuTimeStat {
    CpuTimeStat::from(self.inner.time())
  }

  /// Bandwidth counters, or None when the kernel omitted them.
  #[getter]
  fn bandwidth(&self) -> Option<CpuBandwidthStat> {
    self.inner.bandwidth().map(CpuBandwidthStat::from)
  }
}

/// Local CPU statistics, distinct from hierarchical cpu.stat.
#[pyclass(frozen, module = "sakai._sakai")]
struct CpuStatLocal {
  inner: CoreCpuStatLocal,
}

#[pymethods]
impl CpuStatLocal {
  /// Locally throttled time in nanoseconds, if reported by the kernel.
  #[getter]
  fn throttled_ns(&self) -> Option<u64> {
    self.inner.throttled().map(|time| time.get::<nanosecond>())
  }
}

/// CPU usage times in nanoseconds.
#[pyclass(frozen, module = "sakai._sakai")]
struct CpuTimeStat {
  /// Total CPU usage in nanoseconds.
  #[pyo3(get)]
  usage_ns: u64,
  /// CPU usage in user mode, in nanoseconds.
  #[pyo3(get)]
  user_ns: u64,
  /// CPU usage in kernel mode, in nanoseconds.
  #[pyo3(get)]
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
#[pyclass(frozen, module = "sakai._sakai")]
struct CpuBandwidthStat {
  inner: CoreCpuBandwidthStat,
}

impl From<CoreCpuBandwidthStat> for CpuBandwidthStat {
  fn from(inner: CoreCpuBandwidthStat) -> Self {
    Self { inner }
  }
}

#[pymethods]
impl CpuBandwidthStat {
  /// Number of elapsed bandwidth periods.
  #[getter]
  fn nr_periods(&self) -> u64 {
    self.inner.nr_periods().value
  }

  /// Number of bandwidth periods in which the cgroup was throttled.
  #[getter]
  fn nr_throttled(&self) -> u64 {
    self.inner.nr_throttled().value
  }

  /// Cumulative throttled time in nanoseconds.
  #[getter]
  fn throttled_ns(&self) -> u64 {
    self.inner.throttled().get::<nanosecond>()
  }

  /// Burst counters, if the kernel reports them.
  #[getter]
  fn burst(&self) -> Option<CpuBurstStat> {
    self.inner.burst().map(CpuBurstStat::from)
  }
}

/// CPU bandwidth burst counters.
#[pyclass(frozen, module = "sakai._sakai")]
struct CpuBurstStat {
  /// Number of bursts.
  #[pyo3(get)]
  nr_bursts: u64,
  /// Cumulative burst time in nanoseconds.
  #[pyo3(get)]
  burst_ns: u64,
}

impl From<CoreCpuBurstStat> for CpuBurstStat {
  fn from(value: CoreCpuBurstStat) -> Self {
    Self {
      nr_bursts: value.nr_bursts().value,
      burst_ns: value.burst().get::<nanosecond>(),
    }
  }
}

/// This cgroup's CPU quota and period, independent of ancestor limits.
#[pyclass(frozen, module = "sakai._sakai")]
struct CpuMax {
  /// Quota in nanoseconds or the kernel's `max`.
  #[pyo3(get)]
  quota_ns: MaxOr,
  /// Bandwidth period in nanoseconds.
  #[pyo3(get)]
  period_ns: u64,
  /// Quota divided by period, or `max` at this cgroup only.
  #[pyo3(get)]
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
#[pyclass(frozen, module = "sakai._sakai")]
struct CpuWeight {
  /// Whether this cgroup uses the special idle scheduling weight.
  #[pyo3(get)]
  is_idle: bool,
  /// Weight from 1 through 10,000; None in the idle state.
  #[pyo3(get)]
  shares: Option<u16>,
}

impl From<CoreCpuWeight> for CpuWeight {
  fn from(value: CoreCpuWeight) -> Self {
    match value {
      | CoreCpuWeight::Idle => Self {
        is_idle: true,
        shares: None,
      },
      | CoreCpuWeight::Shares(weight) => Self {
        is_idle: false,
        shares: Some(*weight.as_ref()),
      },
    }
  }
}

/// Pressure-stall information with some and optional full lines.
#[pyclass(frozen, module = "sakai._sakai")]
struct Pressure {
  inner: CorePressure,
}

impl From<CorePressure> for Pressure {
  fn from(inner: CorePressure) -> Self {
    Self { inner }
  }
}

#[pymethods]
impl Pressure {
  /// Time during which some tasks were stalled.
  #[getter]
  fn some(&self) -> PressureLine {
    PressureLine::from(self.inner.some())
  }

  /// Time during which all tasks were stalled, if reported.
  #[getter]
  fn full(&self) -> Option<PressureLine> {
    self.inner.full().map(PressureLine::from)
  }
}

/// Pressure averages as ratios and cumulative stall time in nanoseconds.
#[pyclass(frozen, module = "sakai._sakai")]
struct PressureLine {
  /// Ten-second pressure average, from zero to one.
  #[pyo3(get)]
  avg10: f64,
  /// Sixty-second pressure average, from zero to one.
  #[pyo3(get)]
  avg60: f64,
  /// Three-hundred-second pressure average, from zero to one.
  #[pyo3(get)]
  avg300: f64,
  /// Cumulative stall time in nanoseconds.
  #[pyo3(get)]
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
  owner: Arc<sakai_core::Cgroup>,
}

#[pymethods]
impl MemoryReader {
  /// Return a swap reader that retains this pinned cgroup handle.
  fn swap(&self) -> SwapReader {
    SwapReader {
      owner: Arc::clone(&self.owner),
    }
  }

  /// Return a compressed swap reader that retains this pinned cgroup handle.
  fn zswap(&self) -> ZswapReader {
    ZswapReader {
      owner: Arc::clone(&self.owner),
    }
  }

  /// Read current memory usage in bytes.
  fn current(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().current())
      .map(|value| value.value().get::<byte>())
  }

  /// Read peak memory usage in bytes.
  fn peak(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().peak())
      .map(|value| value.value().get::<byte>())
  }

  /// Read the hard memory limit as `max` or a concrete byte count.
  fn max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().max())
      .map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read the throttling threshold as `max` or a concrete byte count.
  fn high(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().high())
      .map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read the best-effort memory protection boundary in bytes.
  fn low(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().low())
      .map(|value| value.value().get::<byte>())
  }

  /// Read the hard memory protection boundary in bytes.
  fn min(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().min())
      .map(|value| value.value().get::<byte>())
  }

  /// Read memory statistics in separate byte, page, and count mappings.
  fn stat(&self, py: Python<'_>) -> PyResult<MemoryStat> {
    PythonError::read(py, &self.owner, |owner| owner.memory().stat())
      .map(Into::into)
  }

  /// Read memory statistics by field and NUMA node, grouped by native units.
  fn numa_stat(&self, py: Python<'_>) -> PyResult<MemoryNumaStat> {
    PythonError::read(py, &self.owner, |owner| owner.memory().numa_stat())
      .map(Into::into)
  }

  /// Read memory pressure-stall averages and cumulative times.
  fn pressure(&self, py: Python<'_>) -> PyResult<Pressure> {
    PythonError::read(py, &self.owner, |owner| owner.memory().pressure())
      .map(Into::into)
  }
}

/// Fresh reads of swap usage, limits, and events for one pinned cgroup.
#[pyclass(frozen, module = "sakai._sakai")]
struct SwapReader {
  owner: Arc<sakai_core::Cgroup>,
}

#[pymethods]
impl SwapReader {
  /// Read current hierarchical swap usage in bytes.
  fn current(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().current())
      .map(|value| value.value().get::<byte>())
  }

  /// Read peak swap usage since cgroup creation; this never resets the peak.
  fn peak(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().peak())
      .map(|value| value.value().get::<byte>())
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
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().events())
      .map(Into::into)
  }
}

/// Immutable swap threshold and allocation failure counters.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
struct SwapEvents {
  /// Times swap usage exceeded the high threshold; absent on older kernels.
  #[pyo3(get)]
  high: Option<u64>,
  /// Times swap allocation failed at the max boundary.
  #[pyo3(get)]
  max: u64,
  /// Swap allocation failures from the limit or system-wide exhaustion.
  #[pyo3(get)]
  fail: u64,
}

impl From<CoreSwapEvents> for SwapEvents {
  fn from(value: CoreSwapEvents) -> Self {
    Self {
      high: value.high().map(|count| count.value),
      max: value.max().value,
      fail: value.fail().value,
    }
  }
}

/// Fresh reads of compressed swap usage and settings for one pinned cgroup.
#[pyclass(frozen, module = "sakai._sakai")]
struct ZswapReader {
  owner: Arc<sakai_core::Cgroup>,
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
    PythonError::read(py, &self.owner, |owner| {
      owner.memory().zswap().writeback()
    })
    .map(|value| value.value())
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
      bytes: value
        .bytes()
        .iter()
        .map(|(key, value)| ((*key).into(), value.get::<byte>()))
        .collect(),
      pages: value
        .pages()
        .iter()
        .map(|(key, value)| ((*key).into(), value.value))
        .collect(),
      counts: value
        .counts()
        .iter()
        .map(|(key, value)| ((*key).into(), value.value))
        .collect(),
    }
  }
}

#[pymethods]
impl MemoryStat {
  /// Read-only mapping of memory.stat byte-valued fields.
  #[getter]
  fn bytes(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Self::mapping(py, &self.bytes)
  }

  /// Read-only mapping of memory.stat page-valued fields.
  #[getter]
  fn pages(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Self::mapping(py, &self.pages)
  }

  /// Read-only mapping of memory.stat count-valued fields.
  #[getter]
  fn counts(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Self::mapping(py, &self.counts)
  }
}

impl MemoryStat {
  fn mapping(
    py: Python<'_>,
    values: &BTreeMap<&str, u64>,
  ) -> PyResult<Py<PyMappingProxy>> {
    let dict = PyDict::new(py);
    for (key, value) in values {
      dict.set_item(key, value)?;
    }
    Ok(PyMappingProxy::new(py, dict.as_mapping()).unbind())
  }
}

type NumaValues = BTreeMap<&'static str, BTreeMap<u32, u64>>;

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
      bytes: value
        .bytes()
        .iter()
        .map(|(key, nodes)| {
          (
            (*key).into(),
            nodes
              .iter()
              .map(|(node, value)| (*node, value.get::<byte>()))
              .collect(),
          )
        })
        .collect(),
      pages: value
        .pages()
        .iter()
        .map(|(key, nodes)| {
          (
            (*key).into(),
            nodes
              .iter()
              .map(|(node, value)| (*node, value.value))
              .collect(),
          )
        })
        .collect(),
      counts: value
        .counts()
        .iter()
        .map(|(key, nodes)| {
          (
            (*key).into(),
            nodes
              .iter()
              .map(|(node, value)| (*node, value.value))
              .collect(),
          )
        })
        .collect(),
    }
  }
}

#[pymethods]
impl MemoryNumaStat {
  /// Read-only byte amounts by memory field and NUMA node ID.
  #[getter]
  fn bytes(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Self::mapping(py, &self.bytes)
  }

  /// Read-only page quantities by memory field and NUMA node ID.
  #[getter]
  fn pages(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Self::mapping(py, &self.pages)
  }

  /// Read-only event counts by memory field and NUMA node ID.
  #[getter]
  fn counts(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Self::mapping(py, &self.counts)
  }
}

impl MemoryNumaStat {
  fn mapping(
    py: Python<'_>,
    values: &NumaValues,
  ) -> PyResult<Py<PyMappingProxy>> {
    let dict = PyDict::new(py);
    for (key, nodes) in values {
      let inner = PyDict::new(py);
      for (node, value) in nodes {
        inner.set_item(node, value)?;
      }
      dict.set_item(key, PyMappingProxy::new(py, inner.as_mapping()))?;
    }
    Ok(PyMappingProxy::new(py, dict.as_mapping()).unbind())
  }
}

/// Fresh reads of cgroup topology and controller files.
#[pyclass(frozen, module = "sakai._sakai")]
struct CoreReader {
  owner: Arc<sakai_core::Cgroup>,
}

#[pymethods]
impl CoreReader {
  /// Read the cgroup type: domain, domain threaded, domain invalid, or threaded.
  fn kind(&self, py: Python<'_>) -> PyResult<String> {
    PythonError::read(py, &self.owner, |owner| owner.core().kind())
      .map(|kind| kind.to_string())
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

/// Read-only Linux cgroup v2 bindings backed by sakai-core.
#[pymodule]
fn _sakai(module: &Bound<'_, PyModule>) -> PyResult<()> {
  module.add("SakaiError", module.py().get_type::<SakaiError>())?;
  module.add(
    "InterfaceMissingError",
    module.py().get_type::<InterfaceMissingError>(),
  )?;
  module.add(
    "NotCgroupV2Error",
    module.py().get_type::<NotCgroupV2Error>(),
  )?;
  module.add(
    "DeletedCgroupError",
    module.py().get_type::<DeletedCgroupError>(),
  )?;
  module.add(
    "CgroupParseError",
    module.py().get_type::<CgroupParseError>(),
  )?;
  module.add(
    "NotSupportedError",
    module.py().get_type::<NotSupportedError>(),
  )?;
  module.add_class::<Cgroup>()?;
  module.add_class::<MaxOr>()?;
  module.add_class::<CpuReader>()?;
  module.add_class::<MemoryReader>()?;
  module.add_class::<SwapReader>()?;
  module.add_class::<ZswapReader>()?;
  module.add_class::<CoreReader>()?;
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
    exceptions::{PyAttributeError, PyTypeError},
    types::{PyBytes, PyFloat, PyInt},
  };

  use super::*;

  #[test]
  fn converts_cpu_stat_optional_fields_and_nanoseconds() {
    let minimal = CpuStat {
      inner: assert_ok!(
        "usage_usec 10\nuser_usec 7\nsystem_usec 3\n".parse::<CoreCpuStat>()
      ),
    };
    assert_eq!(minimal.time().usage_ns, 10_000);
    assert!(minimal.bandwidth().is_none());

    let complete = CpuStat {
      inner: assert_ok!(
        "usage_usec 10\nuser_usec 7\nsystem_usec 3\nnr_periods \
         5\nnr_throttled 2\nthrottled_usec 4\nnr_bursts 1\nburst_usec 6\n"
          .parse::<CoreCpuStat>()
      ),
    };
    let bandwidth = complete.bandwidth();
    assert_eq!(
      bandwidth.as_ref().map(CpuBandwidthStat::nr_periods),
      Some(5)
    );
    assert_eq!(
      bandwidth.as_ref().map(CpuBandwidthStat::throttled_ns),
      Some(4_000)
    );
    assert_eq!(
      bandwidth
        .and_then(|value| value.burst())
        .map(|value| value.burst_ns),
      Some(6_000)
    );

    let empty_local = CpuStatLocal {
      inner: assert_ok!("".parse::<CoreCpuStatLocal>()),
    };
    let zero_local = CpuStatLocal {
      inner: assert_ok!("throttled_usec 0".parse::<CoreCpuStatLocal>()),
    };
    assert_eq!(empty_local.throttled_ns(), None);
    assert_eq!(zero_local.throttled_ns(), Some(0));
  }

  #[test]
  fn converts_limits_and_idle_without_collapsing_zero() {
    let unlimited =
      CpuMax::from(assert_ok!("max 100000".parse::<CoreCpuMax>()));
    let limited =
      CpuMax::from(assert_ok!("25000 100000".parse::<CoreCpuMax>()));
    assert!(unlimited.quota_ns.is_max());
    assert_err!(unlimited.quota_ns.value());
    assert!(unlimited.cpu_count.is_max());
    assert_err!(unlimited.cpu_count.value());
    assert!(matches!(
      assert_ok!(limited.quota_ns.value()),
      PythonLimitValue::Integer(25_000_000)
    ));
    assert_eq!(limited.period_ns, 100_000_000);
    assert!(matches!(
      assert_ok!(limited.cpu_count.value()),
      PythonLimitValue::Ratio(0.25)
    ));

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
      assert_ok!(
        MaxOr::from_bytes(CoreMaxOr::Value(Bytes::new::<byte>(0))).value()
      ),
      PythonLimitValue::Integer(0)
    ));
  }

  #[test]
  fn max_or_exposes_typed_python_values_and_guards_max() {
    Python::initialize();
    Python::attach(|py| {
      let integer = assert_ok!(Py::new(
        py,
        MaxOr::from_bytes(CoreMaxOr::Value(Bytes::new::<byte>(0)))
      ));
      let integer_value = assert_ok!(integer.bind(py).getattr("value"));
      assert!(integer_value.is_instance_of::<PyInt>());
      assert_eq!(assert_ok!(integer_value.extract::<u64>()), 0);

      let ratio_limit = assert_ok!(Py::new(
        py,
        MaxOr::from_ratio(CoreMaxOr::Value(Ratio::new::<ratio>(0.25)))
      ));
      let ratio_value = assert_ok!(ratio_limit.bind(py).getattr("value"));
      assert!(ratio_value.is_instance_of::<PyFloat>());
      assert_in_delta!(
        assert_ok!(ratio_value.extract::<f64>()),
        0.25,
        f64::EPSILON
      );

      let max = assert_ok!(Py::new(py, MaxOr::from_bytes(CoreMaxOr::Max)));
      let error = max.bind(py).getattr("value").expect_err("max has no value");
      assert!(error.is_instance_of::<PyValueError>(py));
    });
  }

  #[test]
  fn converts_pressure_percent_and_optional_full_line() {
    let pressure = Pressure::from(assert_ok!(
      "some avg10=12.50 avg60=0.00 avg300=100.00 total=23\n"
        .parse::<CorePressure>()
    ));
    assert_in_delta!(pressure.some().avg10, 0.125, f64::EPSILON);
    assert_eq!(pressure.some().total_ns, 23_000);
    assert!(pressure.full().is_none());
  }

  #[test]
  fn separates_memory_bytes_pages_and_counts() {
    let stat = MemoryStat::from(assert_ok!(
      "anon 4096\nfile 2048\npswpin 3\npgfault 7\n".parse::<CoreMemoryStat>()
    ));
    assert_eq!(stat.bytes.get("anon"), Some(&4096));
    assert_eq!(stat.pages.get("pswpin"), Some(&3));
    assert_eq!(stat.counts.get("pgfault"), Some(&7));
    assert!(!stat.bytes.contains_key("pswpin"));
    assert!(!stat.pages.contains_key("pgfault"));
  }

  #[test]
  fn preserves_swap_optional_high_and_unsigned_counts() {
    Python::initialize();
    Python::attach(|py| {
      for (input, expected) in [
        ("max 1\nfail 2\n", SwapEvents {
          high: None,
          max: 1,
          fail: 2,
        }),
        ("high 0\nmax 0\nfail 18446744073709551615\n", SwapEvents {
          high: Some(0),
          max: 0,
          fail: u64::MAX,
        }),
      ] {
        let events =
          SwapEvents::from(assert_ok!(input.parse::<CoreSwapEvents>()));
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
      "anon N0=4096 N2=8192\nfile N0=0\npgdemote_direct \
       N2=3\nworkingset_refault_file N2=7\nfuture N0=nope\n"
        .parse::<CoreMemoryNumaStat>()
    ));
    assert_eq!(stat, MemoryNumaStat {
      bytes: BTreeMap::from([
        ("anon", BTreeMap::from([(0, 4096), (2, 8192)])),
        ("file", BTreeMap::from([(0, 0)])),
      ]),
      pages: BTreeMap::from([("pgdemote_direct", BTreeMap::from([(2, 3)]))]),
      counts: BTreeMap::from([(
        "workingset_refault_file",
        BTreeMap::from([(2, 7)])
      )]),
    });

    Python::initialize();
    Python::attach(|py| {
      let object = assert_ok!(Py::new(py, stat));
      for (name, field, expected) in [
        (
          "bytes",
          "anon",
          BTreeMap::from([(0_u32, 4096_u64), (2, 8192)]),
        ),
        ("pages", "pgdemote_direct", BTreeMap::from([(2, 3)])),
        (
          "counts",
          "workingset_refault_file",
          BTreeMap::from([(2, 7)]),
        ),
      ] {
        let mapping = assert_ok!(object.bind(py).getattr(name));
        let nodes = assert_ok!(mapping.get_item(field));
        let values = assert_ok!(py.get_type::<PyDict>().call1((&nodes,)));
        assert_eq!(
          assert_ok!(values.extract::<BTreeMap<u32, u64>>()),
          expected
        );
        assert!(
          assert_err!(mapping.set_item("future", 0))
            .is_instance_of::<PyTypeError>(py)
        );
        assert!(
          assert_err!(nodes.set_item(0, 0)).is_instance_of::<PyTypeError>(py)
        );
      }
      assert!(
        assert_err!(object.bind(py).setattr("bytes", 0))
          .is_instance_of::<PyAttributeError>(py)
      );
    });
  }

  #[test]
  fn preserves_python_error_categories_paths_and_errno() {
    Python::initialize();
    Python::attach(|py| {
      let path = PathBuf::from("/sys/fs/cgroup/cpu.stat.local");
      let missing = PythonError::result::<()>(
        py,
        Err(Error::FileMissing { path: path.clone() }),
      )
      .expect_err("a missing interface must raise");
      assert!(missing.is_instance_of::<InterfaceMissingError>(py));
      assert!(missing.is_instance_of::<SakaiError>(py));
      assert!(!missing.is_instance_of::<PyOSError>(py));
      let actual = assert_ok!(
        assert_ok!(missing.value(py).getattr("path")).extract::<PathBuf>()
      );
      assert_eq!(actual, path);

      let denied =
        PythonError::from_core(py, Error::Io(io::Error::from_raw_os_error(13)));
      assert!(denied.is_instance_of::<PyOSError>(py));
      let errno = assert_ok!(
        assert_ok!(denied.value(py).getattr("errno")).extract::<i32>()
      );
      assert_eq!(errno, 13);

      let parsed = PythonError::from_core(py, Error::Parse {
        path: PathBuf::from("/sys/fs/cgroup/cpu.max"),
        source: Box::new(io::Error::other("invalid quota")),
      });
      assert!(parsed.is_instance_of::<CgroupParseError>(py));
      assert!(parsed.to_string().contains("invalid quota"));
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
      let decoded = assert_ok!(
        assert_ok!(py.import("os")).call_method1("fsdecode", (name,))
      );
      let extracted = assert_ok!(PythonPath::extract(&decoded));
      assert_eq!(extracted.as_os_str().as_bytes(), raw);
      let pathlike = assert_ok!(py.eval(
        c"type('BytePath', (), {'__fspath__': lambda self: b'child-\\xff'})()",
        None,
        None,
      ));
      let extracted = assert_ok!(PythonPath::extract(&pathlike));
      assert_eq!(extracted.as_os_str().as_bytes(), raw);

      let error = PythonError::from_core(py, Error::FileMissing {
        path: PathBuf::from(OsString::from_vec(
          b"/sys/fs/cgroup/child-\xff/cpu.stat.local".to_vec(),
        )),
      });
      let path = assert_ok!(error.value(py).getattr("path"));
      let encoded = assert_ok!(
        assert_ok!(py.import("os")).call_method1("fsencode", (path,))
      );
      assert_eq!(
        assert_ok!(encoded.cast::<PyBytes>()).as_bytes(),
        b"/sys/fs/cgroup/child-\xff/cpu.stat.local"
      );
    });
  }
}
