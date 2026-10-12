//! Core bindings and conversions.

use std::{collections::BTreeMap, sync::Arc};

use pyo3::{
  prelude::*,
  types::{IntoPyDict, PyMappingProxy},
};
use sakai::v2::core::{
  CgroupEvents as CoreCgroupEvents, CgroupStat as CoreCgroupStat, CgroupStatLocal as CoreCgroupStatLocal,
};
use uom::si::time::nanosecond;

use super::{MaxOr, PythonError};

/// Immutable lifecycle snapshot; states can change immediately after a read.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CgroupEvents {
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
/// Subsystem counts, available since Linux 6.12, include this cgroup and its
/// descendants. Properties inspect stored snapshots without performing I/O.
/// See https://github.com/torvalds/linux/blob/v6.12/kernel/cgroup/cgroup.c.
#[pyclass(frozen, module = "sakai._sakai")]
#[derive(Debug, PartialEq, Eq)]
pub(super) struct CgroupStat {
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

/// Immutable local freezer accounting from cgroup.stat.local, since Linux 6.18.
///
/// Cumulative duration includes ancestor-imposed freezing and an active freeze
/// interval. It starts when freezing is requested, without waiting for the
/// frozen state; it is neither CPU time nor the sum of descendants' durations.
/// Properties inspect stored snapshots without I/O.
/// See https://github.com/torvalds/linux/blob/v6.18/kernel/cgroup/cgroup.c.
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CgroupStatLocal {
  /// Stored cumulative local freezing duration in integer nanoseconds.
  frozen_time_ns: u64,
}

impl From<CoreCgroupStatLocal> for CgroupStatLocal {
  fn from(value: CoreCgroupStatLocal) -> Self { Self { frozen_time_ns: value.frozen_time().get::<nanosecond>() } }
}

/// Fresh reads of cgroup topology, membership, configuration, and lifecycle files.
///
/// Each call performs read-only I/O; separate calls are not atomic. Missing
/// interfaces raise InterfaceMissingError, malformed contents raise
/// CgroupParseError, unsupported reads raise NotSupportedError, and other
/// operating-system failures raise OSError with their cause and interface path.
/// See https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct CoreReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl CoreReader {
  /// Read process IDs in the reader's PID namespace, preserving order and duplicates.
  ///
  /// Excludes descendants and zombies. A threaded domain reports its entire
  /// threaded subtree; reading from a threaded cgroup raises NotSupportedError.
  /// IDs may be recycled during a read and do not pin processes. Zero entries
  /// represent tasks invisible in the reader's PID namespace. See CoreReader
  /// for other read errors and the Linux interface reference.
  fn procs(&self, py: Python<'_>) -> PyResult<Vec<i32>> {
    PythonError::read(py, &self.owner, |owner| owner.core().procs())
      .map(|values| values.into_iter().map(|value| value.into_inner()).collect())
  }

  /// Read this cgroup's thread IDs in the reader's PID namespace.
  ///
  /// Excludes descendants, preserves order and possible duplicates, and accepts
  /// empty membership. Zero entries represent namespace-invisible tasks.
  /// IDs can be recycled while reading. See CoreReader for
  /// read errors and the Linux interface reference.
  fn threads(&self, py: Python<'_>) -> PyResult<Vec<i32>> {
    PythonError::read(py, &self.owner, |owner| owner.core().threads())
      .map(|values| values.into_iter().map(|value| value.into_inner()).collect())
  }

  /// Read volatile populated and frozen state; root cgroups lack this interface.
  fn events(&self, py: Python<'_>) -> PyResult<CgroupEvents> {
    PythonError::read(py, &self.owner, |owner| owner.core().events()).map(Into::into)
  }

  /// Read volatile descendant and subsystem counts, including at the hierarchy root.
  fn stat(&self, py: Python<'_>) -> PyResult<CgroupStat> {
    PythonError::read(py, &self.owner, |owner| owner.core().stat()).map(Into::into)
  }

  /// Read cumulative local freezing time, including ancestor-imposed freezing.
  ///
  /// Since Linux 6.18; the root lacks this interface. See CgroupStatLocal for
  /// timing semantics and CoreReader for read errors.
  fn stat_local(&self, py: Python<'_>) -> PyResult<CgroupStatLocal> {
    PythonError::read(py, &self.owner, |owner| owner.core().stat_local()).map(Into::into)
  }

  /// Read this cgroup's own freeze request as bool, excluding ancestor requests.
  ///
  /// This is configuration, not completed freezing; use events().frozen for
  /// observed state. The root lacks this file. See CoreReader for read errors.
  fn freeze(&self, py: Python<'_>) -> PyResult<bool> {
    PythonError::read(py, &self.owner, |owner| owner.core().freeze()).map(|value| value.value())
  }

  /// Read the configured maximum descendant depth as MaxOr[int] cgroup levels.
  ///
  /// Zero prevents children; max removes this cgroup's restriction. Ancestor
  /// restrictions still apply. Available at the root. See CoreReader for errors.
  fn max_depth(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.core().max_depth()).map(|value| MaxOr::from_count(value.value()))
  }

  /// Read the maximum descendant cgroup count, excluding this cgroup, as MaxOr[int].
  ///
  /// Zero prevents children; max removes this cgroup's restriction. Ancestors
  /// still restrict creation, and existing descendants may exceed a lowered
  /// limit. Available at the root. See CoreReader for read errors.
  fn max_descendants(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.core().max_descendants())
      .map(|value| MaxOr::from_count(value.value()))
  }

  /// Read whether pressure stall accounting is enabled locally as bool.
  ///
  /// Since Linux 6.1; requires CONFIG_PSI and enabled kernel PSI support.
  /// Disabling accounting here does not disable descendants. Available at the
  /// root. See CoreReader for read errors and the Linux interface reference.
  fn pressure_enabled(&self, py: Python<'_>) -> PyResult<bool> {
    PythonError::read(py, &self.owner, |owner| owner.core().pressure_enabled()).map(|value| value.value())
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

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use pyo3::{
    exceptions::{PyAttributeError, PyTypeError},
    types::{PyDict, PyInt},
  };
  use sakai::v2::pids::PidsEvents as CorePidsEvents;

  use super::{
    super::{MaxOr, PidsEvents},
    *,
  };

  #[test]
  fn preserves_process_limits_states_and_immutable_subsystem_counts() {
    Python::initialize();
    Python::attach(|py| {
      let local = CgroupStatLocal::from(assert_ok!("frozen_usec 123456".parse::<CoreCgroupStatLocal>()));
      assert_eq!(local, CgroupStatLocal { frozen_time_ns: 123_456_000 });
      let object = assert_ok!(Py::new(py, local));
      assert_eq!(assert_ok!(assert_ok!(object.bind(py).getattr("frozen_time_ns")).extract::<u64>()), 123_456_000);
      assert!(assert_err!(object.bind(py).setattr("frozen_time_ns", 0)).is_instance_of::<PyAttributeError>(py));
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
}
