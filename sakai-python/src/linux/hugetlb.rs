//! Python HugeTLB readers and immutable event and NUMA snapshots.

use std::{collections::BTreeMap, sync::Arc};

use pyo3::{
  exceptions::PyValueError,
  prelude::*,
  types::{IntoPyDict, PyMappingProxy},
};
use sakai::v2::hugetlb::{HugetlbEvents as CoreHugetlbEvents, HugetlbNumaStat as CoreHugetlbNumaStat, PageSize};
use uom::si::information::byte;

use super::{MaxOr, PythonError};

/// Fresh HugeTLB reads by page-size label for one pinned cgroup.
///
/// Labels such as "64KB", "2MB", and "1GB" use binary units and the kernel's
/// uppercase canonical spelling. Invalid labels raise ValueError before I/O;
/// a valid size unsupported by this kernel raises InterfaceMissingError.
/// All HugeTLB files are absent on the root cgroup. Disabled controllers,
/// omitted kernel features, and older kernels also raise InterfaceMissingError
/// with the interface path. Invalid file contents raise CgroupParseError,
/// unsupported reads raise NotSupportedError, and other OS failures raise
/// OSError. Every call performs fresh read-only I/O; separate calls are not
/// atomic. No pages or reservations are allocated by reads.
///
/// HugeTLB is distinct from transparent huge pages. See Linux documentation:
/// https://docs.kernel.org/admin-guide/cgroup-v2.html#hugetlb
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct HugetlbReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

impl HugetlbReader {
  fn size(label: &str) -> PyResult<PageSize> {
    label.parse().map_err(|error| PyValueError::new_err(format!("invalid HugeTLB page size: {error}")))
  }
}

#[pymethods]
impl HugetlbReader {
  /// Read volatile faulted HugeTLB usage for page_size in bytes, including descendants.
  ///
  /// page_size is a canonical kernel label, for example "2MB". This reads
  /// hugetlb.<page_size>.current and returns a Python integer without unit conversion.
  fn current(&self, py: Python<'_>, page_size: &str) -> PyResult<u64> {
    let size = Self::size(page_size)?;
    PythonError::read(py, &self.owner, move |owner| owner.hugetlb().current(&size))
      .map(|value| value.value().get::<byte>())
  }

  /// Read the faulted HugeTLB limit for page_size as MaxOr[int] in bytes.
  ///
  /// This reads hugetlb.<page_size>.max. max means no limit at this cgroup;
  /// ancestors still constrain usage. Configuration may change between reads.
  fn max(&self, py: Python<'_>, page_size: &str) -> PyResult<MaxOr> {
    let size = Self::size(page_size)?;
    PythonError::read(py, &self.owner, move |owner| owner.hugetlb().max(&size))
      .map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read reservation-accounting usage for page_size in bytes, including descendants.
  ///
  /// hugetlb.<page_size>.rsvd.current charges reservations before faulting and
  /// also counts faulted pages without prior reservations. It does not measure
  /// only unused reservations. Usage is volatile.
  fn reserved_current(&self, py: Python<'_>, page_size: &str) -> PyResult<u64> {
    let size = Self::size(page_size)?;
    PythonError::read(py, &self.owner, move |owner| owner.hugetlb().reserved_current(&size))
      .map(|value| value.value().get::<byte>())
  }

  /// Read the reservation-accounting limit for page_size as MaxOr[int] in bytes.
  ///
  /// hugetlb.<page_size>.rsvd.max limits reservations and faults without prior
  /// reservations. max means no limit here; ancestor limits still apply.
  fn reserved_max(&self, py: Python<'_>, page_size: &str) -> PyResult<MaxOr> {
    let size = Self::size(page_size)?;
    PythonError::read(py, &self.owner, move |owner| owner.hugetlb().reserved_max(&size))
      .map(|value| MaxOr::from_bytes(value.value()))
  }

  /// Read cumulative HugeTLB limit failures for page_size, including descendants.
  ///
  /// Both fault and reservation limits contribute to hugetlb.<page_size>.events.
  fn events(&self, py: Python<'_>, page_size: &str) -> PyResult<HugetlbEvents> {
    let size = Self::size(page_size)?;
    PythonError::read(py, &self.owner, move |owner| owner.hugetlb().events(&size)).map(Into::into)
  }

  /// Read cumulative HugeTLB limit failures originating in this cgroup alone.
  ///
  /// This reads hugetlb.<page_size>.events.local; descendant failures are excluded.
  fn events_local(&self, py: Python<'_>, page_size: &str) -> PyResult<HugetlbEvents> {
    let size = Self::size(page_size)?;
    PythonError::read(py, &self.owner, move |owner| owner.hugetlb().events_local(&size)).map(Into::into)
  }

  /// Read active HugeTLB bytes for page_size by NUMA node, including descendants.
  ///
  /// hugetlb.<page_size>.numa_stat includes only in-use pages, not unused
  /// reservations. Since Linux 5.17; absent on older kernels. Total and node
  /// counters can change during the kernel read, so their sum need not agree.
  /// https://github.com/torvalds/linux/blob/v5.17/mm/hugetlb_cgroup.c
  fn numa_stat(&self, py: Python<'_>, page_size: &str) -> PyResult<HugetlbNumaStat> {
    let size = Self::size(page_size)?;
    PythonError::read(py, &self.owner, move |owner| owner.hugetlb().numa_stat(&size)).map(Into::into)
  }
}

/// Immutable cumulative HugeTLB limit-failure counts for one page size.
///
/// events() includes descendants; events_local() excludes them. Both fault
/// and reservation limit failures contribute. Counts are volatile and stored
/// getters never perform I/O. Unknown future keys are ignored.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HugetlbEvents {
  /// Stored count of HugeTLB allocation failures caused by a cgroup limit.
  max: u64,
}

impl From<CoreHugetlbEvents> for HugetlbEvents {
  fn from(value: CoreHugetlbEvents) -> Self { Self { max: value.max().value } }
}

/// Immutable hierarchical in-use HugeTLB byte distribution for one page size.
///
/// Since Linux 5.17. Unused reservations are excluded. Kernel reads of total
/// and individual node counters are not atomic, so their sum need not agree.
/// Properties inspect this stored snapshot; they never perform I/O.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct HugetlbNumaStat {
  /// Stored total in-use HugeTLB bytes across this cgroup and descendants.
  #[pyo3(get)]
  total: u64,
  nodes: BTreeMap<u32, u64>,
}

impl From<CoreHugetlbNumaStat> for HugetlbNumaStat {
  fn from(value: CoreHugetlbNumaStat) -> Self {
    Self {
      total: value.total().get::<byte>(),
      nodes: value.nodes().iter().map(|(node, value)| (*node, value.get::<byte>())).collect(),
    }
  }
}

#[pymethods]
impl HugetlbNumaStat {
  /// Read-only mapping from integer NUMA node IDs to stored in-use bytes.
  #[getter]
  fn nodes(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, (&self.nodes).into_py_dict(py)?.as_mapping()).unbind())
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use pyo3::types::PyDict;

  use super::{
    super::tests::{check_frozen, readonly},
    *,
  };

  #[test]
  fn converts_snapshots_to_immutable_unsigned_python_values() {
    Python::initialize();
    Python::attach(|py| {
      let events = HugetlbEvents::from(assert_ok!("max 18446744073709551615".parse::<CoreHugetlbEvents>()));
      assert_eq!(events, HugetlbEvents { max: u64::MAX });
      let object = assert_ok!(Py::new(py, events));
      assert_eq!(assert_ok!(assert_ok!(object.bind(py).getattr("max")).extract::<u64>()), u64::MAX);
      check_frozen(object.bind(py).as_any(), &["max"]);

      let expected = HugetlbNumaStat { total: u64::MAX, nodes: BTreeMap::from([(0, 0), (7, u64::MAX)]) };
      let snapshot = HugetlbNumaStat::from(assert_ok!(
        "N7=18446744073709551615 total=18446744073709551615 N0=0".parse::<CoreHugetlbNumaStat>()
      ));
      assert_eq!(snapshot, expected);
      let object = assert_ok!(Py::new(py, snapshot));
      assert_eq!(assert_ok!(assert_ok!(object.bind(py).getattr("total")).extract::<u64>()), expected.total);
      let nodes = readonly(object.bind(py).as_any(), "nodes", 0);
      let values = assert_ok!(py.get_type::<PyDict>().call1((&nodes,)));
      assert_eq!(assert_ok!(values.extract::<BTreeMap<u32, u64>>()), expected.nodes);
      check_frozen(object.bind(py).as_any(), &["total"]);
      for label in ["../2MB", "0MB", "2mb", "3MB", "2048KB"] {
        assert!(assert_err!(HugetlbReader::size(label)).is_instance_of::<PyValueError>(py));
      }
    });
  }
}
