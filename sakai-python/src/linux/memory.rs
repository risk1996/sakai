//! Memory bindings and conversions.

use std::{collections::BTreeMap, sync::Arc};

use pyo3::{
  prelude::*,
  types::{IntoPyDict, PyDict, PyMappingProxy},
};
use sakai::v2::memory::{
  MemoryEventCounts as CoreMemoryEventCounts, MemoryNumaStat as CoreMemoryNumaStat, MemoryStat as CoreMemoryStat,
  SwapEvents as CoreSwapEvents,
};
use uom::si::information::byte;

use super::{MaxOr, Pressure, PythonError};

/// Fresh reads of memory controller files for one pinned cgroup.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MemoryReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl MemoryReader {
  /// Return a swap reader that retains this pinned cgroup handle.
  fn swap(&self) -> SwapReader { SwapReader { owner: Arc::clone(&self.owner) } }

  /// Return a compressed swap reader that retains this pinned cgroup handle.
  fn zswap(&self) -> ZswapReader { ZswapReader { owner: Arc::clone(&self.owner) } }

  /// Read current memory usage of this cgroup and its descendants, in bytes.
  fn current(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().current()).map(|value| value.value().get::<byte>())
  }

  /// Read peak hierarchical memory usage since cgroup creation, in bytes.
  /// This fresh read-only descriptor never resets the peak.
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

  /// Read best-effort memory protection as `max` or a concrete byte count.
  /// Ancestor boundaries and sibling competition affect effective protection.
  fn low(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().low()).map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read hard memory protection as `max` or a concrete byte count.
  /// Ancestor boundaries and sibling competition affect effective protection.
  fn min(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().min()).map(|value| MaxOr::from_bytes(value.value()))
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
pub(super) struct MemoryEventCounts {
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
pub(super) struct SwapReader {
  pub(super) owner: Arc<sakai::Cgroup>,
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

  /// Read swap threshold and allocation failure counters for this cgroup and descendants.
  fn events(&self, py: Python<'_>) -> PyResult<SwapEvents> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().events()).map(Into::into)
  }
}

/// Immutable swap threshold and allocation failure counters, including descendants.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SwapEvents {
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
pub(super) struct ZswapReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl ZswapReader {
  /// Read zswap compression-backend memory for this cgroup and descendants, in bytes.
  fn current(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, |owner| owner.memory().zswap().current())
      .map(|value| value.value().get::<byte>())
  }

  /// Read the hard zswap pool limit as `max` or a concrete byte count.
  fn max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().zswap().max())
      .map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read this cgroup's configured disk writeback policy.
  /// Since Linux 6.11, an ancestor can disable effective writeback. The policy
  /// does not block disk swap when zswap is disabled globally.
  fn writeback(&self, py: Python<'_>) -> PyResult<bool> {
    PythonError::read(py, &self.owner, |owner| owner.memory().zswap().writeback()).map(|value| value.value())
  }
}

/// Memory statistics grouped by their native units.
///
/// Keys retain the kernel's snake_case names. Unknown kernel fields are
/// ignored; each exposed mapping is read-only. Missing fields differ from zero.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MemoryStat {
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
pub(super) struct MemoryValues;

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

/// Immutable per-NUMA-node memory statistics for this cgroup and descendants.
///
/// Outer keys retain kernel field names; inner keys are integer NUMA node IDs.
/// Both levels of every mapping are read-only. Unknown fields are ignored.
/// Only fields with per-node accounting are reported; missing values differ from zero.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct MemoryNumaStat {
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

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use pyo3::exceptions::{PyAttributeError, PyTypeError};

  use super::*;

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
}
