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

use super::{MaxOr, Pressure, PythonError};
use crate::quantity::Quantity;

/// Fresh reads of memory controller files for one pinned cgroup.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MemoryReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl MemoryReader {
  /// Return a swap reader that retains this pinned cgroup handle.
  fn swap(&self) -> SwapReader { SwapReader { owner: Arc::clone(&self.owner) } }

  /// Return a compressed swap reader that retains this pinned cgroup handle.
  fn zswap(&self) -> ZswapReader { ZswapReader { owner: Arc::clone(&self.owner) } }

  /// Read current memory usage of this cgroup and its descendants, in bytes.
  fn current(&self, py: Python<'_>) -> PyResult<Quantity<sakai::Bytes>> {
    PythonError::read(py, &self.owner, |owner| owner.memory().current()).map(|value| value.value().into())
  }

  /// Read peak hierarchical memory usage since cgroup creation, in bytes.
  /// This fresh read-only descriptor never resets the peak.
  fn peak(&self, py: Python<'_>) -> PyResult<Quantity<sakai::Bytes>> {
    PythonError::read(py, &self.owner, |owner| owner.memory().peak()).map(|value| value.value().into())
  }

  /// Read the hard memory limit as `max` or a concrete byte count.
  #[gen_stub(override_return_type(type_repr = "MaxOr[pint.Quantity[int]]"))]
  fn max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().max()).map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read the throttling threshold as `max` or a concrete byte count.
  #[gen_stub(override_return_type(type_repr = "MaxOr[pint.Quantity[int]]"))]
  fn high(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().high()).map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read best-effort memory protection as `max` or a concrete byte count.
  /// Ancestor boundaries and sibling competition affect effective protection.
  #[gen_stub(override_return_type(type_repr = "MaxOr[pint.Quantity[int]]"))]
  fn low(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().low()).map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read hard memory protection as `max` or a concrete byte count.
  /// Ancestor boundaries and sibling competition affect effective protection.
  #[gen_stub(override_return_type(type_repr = "MaxOr[pint.Quantity[int]]"))]
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
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemoryEventCounts {
  /// Reclaims despite usage below the effective low boundary.
  low: Quantity<sakai::Count>,
  /// Throttling and direct reclaim after crossing the high boundary.
  high: Quantity<sakai::Count>,
  /// Attempts to cross the max boundary.
  max: Quantity<sakai::Count>,
  /// Allocations about to fail at the memory limit.
  oom: Quantity<sakai::Count>,
  /// Processes killed by any OOM killer.
  oom_kill: Quantity<sakai::Count>,
  /// Group OOM kills, or None when the kernel omits the counter.
  oom_group_kill: Option<Quantity<sakai::Count>>,
  /// Network socket throttling events, or None when the kernel omits the counter.
  sock_throttled: Option<Quantity<sakai::Count>>,
}

impl From<CoreMemoryEventCounts> for MemoryEventCounts {
  fn from(value: CoreMemoryEventCounts) -> Self {
    Self {
      low: value.low().into(),
      high: value.high().into(),
      max: value.max().into(),
      oom: value.oom().into(),
      oom_kill: value.oom_kill().into(),
      oom_group_kill: value.oom_group_kill().map(|count| count.into()),
      sock_throttled: value.sock_throttled().map(|count| count.into()),
    }
  }
}

/// Fresh reads of swap usage, limits, and events for one pinned cgroup.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct SwapReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl SwapReader {
  /// Read current hierarchical swap usage in bytes.
  fn current(&self, py: Python<'_>) -> PyResult<Quantity<sakai::Bytes>> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().current()).map(|value| value.value().into())
  }

  /// Read peak swap usage since cgroup creation; this never resets the peak.
  fn peak(&self, py: Python<'_>) -> PyResult<Quantity<sakai::Bytes>> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().peak()).map(|value| value.value().into())
  }

  /// Read the hard swap limit as `max` or a concrete byte count.
  #[gen_stub(override_return_type(type_repr = "MaxOr[pint.Quantity[int]]"))]
  fn max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().max())
      .map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read the swap throttling threshold as `max` or a concrete byte count.
  #[gen_stub(override_return_type(type_repr = "MaxOr[pint.Quantity[int]]"))]
  fn high(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().high())
      .map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read swap threshold and allocation failure counters for this cgroup and descendants.
  ///
  /// With the cgroup2 memory_localevents mount option, only this cgroup's events
  /// are reported; parent counters exclude descendant failures.
  fn events(&self, py: Python<'_>) -> PyResult<SwapEvents> {
    PythonError::read(py, &self.owner, |owner| owner.memory().swap().events()).map(Into::into)
  }
}

/// Immutable swap threshold and allocation failure counters, including descendants.
///
/// With the cgroup2 memory_localevents mount option, events do not propagate to
/// ancestors and these counters describe only the cgroup that was read.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SwapEvents {
  /// Times swap usage exceeded the high threshold; absent on older kernels.
  high: Option<Quantity<sakai::Count>>,
  /// Times swap allocation failed at the max boundary.
  max: Quantity<sakai::Count>,
  /// Swap allocation failures from the limit or system-wide exhaustion.
  fail: Quantity<sakai::Count>,
}

impl From<CoreSwapEvents> for SwapEvents {
  fn from(value: CoreSwapEvents) -> Self {
    Self { high: value.high().map(|count| count.into()), max: value.max().into(), fail: value.fail().into() }
  }
}

/// Fresh reads of compressed swap usage and settings for one pinned cgroup.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct ZswapReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl ZswapReader {
  /// Read zswap compression-backend memory for this cgroup and descendants, in bytes.
  fn current(&self, py: Python<'_>) -> PyResult<Quantity<sakai::Bytes>> {
    PythonError::read(py, &self.owner, |owner| owner.memory().zswap().current()).map(|value| value.value().into())
  }

  /// Read the hard zswap pool limit as `max` or a concrete byte count.
  #[gen_stub(override_return_type(type_repr = "MaxOr[pint.Quantity[int]]"))]
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
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MemoryStat {
  bytes: BTreeMap<&'static str, Quantity<sakai::Bytes>>,
  pages: BTreeMap<&'static str, Quantity<sakai::Pages>>,
  counts: BTreeMap<&'static str, Quantity<sakai::Count>>,
}

impl From<CoreMemoryStat> for MemoryStat {
  fn from(value: CoreMemoryStat) -> Self {
    Self {
      bytes: MemoryValues::fields(value.bytes(), |value| value.into()),
      pages: MemoryValues::fields(value.pages(), |value| value.into()),
      counts: MemoryValues::fields(value.counts(), |value| value.into()),
    }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl MemoryStat {
  /// Read-only mapping of memory.stat byte-valued fields.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[str, pint.Quantity[int]]", imports = ("collections.abc", "pint",)))]
  fn bytes(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.bytes) }

  /// Read-only mapping of memory.stat page-valued fields.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[str, pint.Quantity[int]]", imports = ("collections.abc", "pint",)))]
  fn pages(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.pages) }

  /// Read-only mapping of memory.stat count-valued fields.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[str, pint.Quantity[int]]", imports = ("collections.abc", "pint",)))]
  fn counts(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.counts) }
}

impl MemoryStat {
  fn mapping<Q>(py: Python<'_>, values: &BTreeMap<&str, Quantity<Q>>) -> PyResult<Py<PyMappingProxy>>
  where
    for<'py> &'py Quantity<Q>: IntoPyObject<'py>, {
    Ok(PyMappingProxy::new(py, values.into_py_dict(py)?.as_mapping()).unbind())
  }
}

type NumaValues<Q> = BTreeMap<&'static str, BTreeMap<u32, Quantity<Q>>>;

/// Converts memory field names and quantities without assuming a page size.
pub(super) struct MemoryValues;

impl MemoryValues {
  fn fields<Field: Copy + Into<&'static str>, Value, Output>(
    values: &BTreeMap<Field, Value>,
    convert: impl Fn(&Value) -> Output,
  ) -> BTreeMap<&'static str, Output> {
    values.iter().map(|(key, value)| ((*key).into(), convert(value))).collect()
  }

  fn nodes<Value, Q>(
    values: &BTreeMap<u32, Value>,
    convert: impl Fn(&Value) -> Quantity<Q>,
  ) -> BTreeMap<u32, Quantity<Q>> {
    values.iter().map(|(node, value)| (*node, convert(value))).collect()
  }
}

/// Immutable per-NUMA-node memory statistics for this cgroup and descendants.
///
/// Outer keys retain kernel field names; inner keys are integer NUMA node IDs.
/// Both levels of every mapping are read-only. Unknown fields are ignored.
/// Only fields with per-node accounting are reported; missing values differ from zero.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct MemoryNumaStat {
  bytes: NumaValues<sakai::Bytes>,
  pages: NumaValues<sakai::Pages>,
  counts: NumaValues<sakai::Count>,
}

impl From<CoreMemoryNumaStat> for MemoryNumaStat {
  fn from(value: CoreMemoryNumaStat) -> Self {
    Self {
      bytes: MemoryValues::fields(value.bytes(), |nodes| MemoryValues::nodes(nodes, |value| value.into())),
      pages: MemoryValues::fields(value.pages(), |nodes| MemoryValues::nodes(nodes, |value| value.into())),
      counts: MemoryValues::fields(value.counts(), |nodes| MemoryValues::nodes(nodes, |value| value.into())),
    }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl MemoryNumaStat {
  /// Read-only byte amounts by memory field and NUMA node ID.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[str, collections.abc.Mapping[int, pint.Quantity[int]]]", imports = ("collections.abc", "pint",)))]
  fn bytes(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.bytes) }

  /// Read-only page quantities by memory field and NUMA node ID.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[str, collections.abc.Mapping[int, pint.Quantity[int]]]", imports = ("collections.abc", "pint",)))]
  fn pages(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.pages) }

  /// Read-only event counts by memory field and NUMA node ID.
  #[getter]
  #[gen_stub(override_return_type(type_repr = "collections.abc.Mapping[str, collections.abc.Mapping[int, pint.Quantity[int]]]", imports = ("collections.abc", "pint",)))]
  fn counts(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> { Self::mapping(py, &self.counts) }
}

impl MemoryNumaStat {
  fn mapping<Q>(py: Python<'_>, values: &NumaValues<Q>) -> PyResult<Py<PyMappingProxy>>
  where
    for<'py> &'py Quantity<Q>: IntoPyObject<'py>, {
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
  use pyo3::exceptions::PyTypeError;

  use super::{
    super::tests::{check_frozen, readonly},
    *,
  };

  #[test]
  fn separates_memory_bytes_pages_and_counts() {
    let stat = MemoryStat::from(assert_ok!("anon 4096\nfile 2048\npswpin 3\npgfault 7\n".parse::<CoreMemoryStat>()));
    assert_eq!(stat.bytes.get("anon"), Some(&4096.into()));
    assert_eq!(stat.pages.get("pswpin"), Some(&3.into()));
    assert_eq!(stat.counts.get("pgfault"), Some(&7.into()));
    assert!(!stat.bytes.contains_key("pswpin"));
    assert!(!stat.pages.contains_key("pgfault"));
  }

  #[test]
  fn preserves_memory_optional_events_zero_and_unsigned_counts() {
    crate::quantity::tests::initialize_python();
    Python::attach(|py| {
      for (input, expected) in [
        ("low 1\nhigh 2\nmax 3\noom 4\noom_kill 5", MemoryEventCounts {
          low: 1.into(),
          high: 2.into(),
          max: 3.into(),
          oom: 4.into(),
          oom_kill: 5.into(),
          oom_group_kill: None,
          sock_throttled: None,
        }),
        (
          "low 0\nhigh 0\nmax 0\noom 0\noom_kill 18446744073709551615\noom_group_kill 0\nsock_throttled 0\nfuture nope",
          MemoryEventCounts {
            low: 0.into(),
            high: 0.into(),
            max: 0.into(),
            oom: 0.into(),
            oom_kill: u64::MAX.into(),
            oom_group_kill: Some(0.into()),
            sock_throttled: Some(0.into()),
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
          assert_eq!(
            assert_ok!(assert_ok!(object.bind(py).getattr("oom_kill")).extract::<Quantity<sakai::Count>>()),
            expected.oom_kill
          );
          for (name, expected) in
            [("oom_group_kill", expected.oom_group_kill), ("sock_throttled", expected.sock_throttled)]
          {
            assert_eq!(
              assert_ok!(assert_ok!(object.bind(py).getattr(name)).extract::<Option<Quantity<sakai::Count>>>()),
              expected
            );
          }
          check_frozen(object.bind(py).as_any(), &["low"]);
        }
      }
    });
  }

  #[test]
  fn preserves_swap_optional_high_and_unsigned_counts() {
    crate::quantity::tests::initialize_python();
    Python::attach(|py| {
      for (input, expected) in [
        ("max 1\nfail 2\n", SwapEvents { high: None, max: 1.into(), fail: 2.into() }),
        ("high 0\nmax 0\nfail 18446744073709551615\n", SwapEvents {
          high: Some(0.into()),
          max: 0.into(),
          fail: u64::MAX.into(),
        }),
      ] {
        let events = SwapEvents::from(assert_ok!(input.parse::<CoreSwapEvents>()));
        assert_eq!(events, expected);
        let object = assert_ok!(Py::new(py, events));
        let high = assert_ok!(object.bind(py).getattr("high"));
        assert_eq!(assert_ok!(high.extract::<Option<Quantity<sakai::Count>>>()), expected.high);
        check_frozen(object.bind(py).as_any(), &["fail"]);
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
      bytes: BTreeMap::from([
        ("anon", BTreeMap::from([(0, 4096.into()), (2, 8192.into())])),
        ("file", BTreeMap::from([(0, 0.into())])),
      ]),
      pages: BTreeMap::from([("pgdemote_direct", BTreeMap::from([(2, 3.into())]))]),
      counts: BTreeMap::from([("workingset_refault_file", BTreeMap::from([(2, 7.into())]))]),
    });

    crate::quantity::tests::initialize_python();
    Python::attach(|py| {
      let object = assert_ok!(Py::new(py, stat));
      for (name, field, expected) in [
        ("bytes", "anon", BTreeMap::from([(0_u32, 4096_u64), (2, 8192)])),
        ("pages", "pgdemote_direct", BTreeMap::from([(2, 3)])),
        ("counts", "workingset_refault_file", BTreeMap::from([(2, 7)])),
      ] {
        let mapping = readonly(object.bind(py).as_any(), name, "future");
        let nodes = assert_ok!(mapping.get_item(field));
        let values = assert_ok!(py.get_type::<PyDict>().call1((&nodes,)));
        assert_eq!(assert_ok!(super::super::tests::magnitudes(&values).extract::<BTreeMap<u32, u64>>()), expected);
        assert!(assert_err!(nodes.set_item(0, 0)).is_instance_of::<PyTypeError>(py));
      }
    });
  }
}
