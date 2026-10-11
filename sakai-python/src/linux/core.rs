//! Core bindings and conversions.

use std::{collections::BTreeMap, sync::Arc};

use pyo3::{
  prelude::*,
  types::{IntoPyDict, PyMappingProxy},
};
use sakai::v2::core::{CgroupEvents as CoreCgroupEvents, CgroupStat as CoreCgroupStat};

use super::PythonError;

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

/// Fresh reads of cgroup topology, controller, and lifecycle files.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct CoreReader {
  pub(super) owner: Arc<sakai::Cgroup>,
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
