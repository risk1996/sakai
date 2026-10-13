//! Python miscellaneous-resource snapshots and fresh readers.

use std::{collections::BTreeMap, sync::Arc};

use pyo3::{
  prelude::*,
  types::{IntoPyDict, PyDict, PyMappingProxy},
};
use sakai::v2::misc::{
  MiscCapacity as CoreCapacity, MiscCurrent as CoreCurrent, MiscEvents as CoreEvents, MiscMax as CoreMax,
  MiscPeak as CorePeak,
};

use super::{MaxOr, PythonError, limit::PythonLimitValue};

/// Fresh read-only miscellaneous-resource snapshots for a pinned cgroup.
///
/// Each method performs separate I/O. Missing files, including controller
/// availability, older kernels and root exemptions, raise InterfaceMissingError;
/// invalid contents raise CgroupParseError, unsupported operations raise
/// NotSupportedError and other OS failures raise OSError. Resource values count
/// hardware-specific units, never bytes. See https://docs.kernel.org/admin-guide/cgroup-v2.html#misc.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MiscReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[pymethods]
impl MiscReader {
  /// Read machine-wide resource capacities; exists only at the hierarchy root.
  ///
  /// Performs fresh read-only I/O with the exception policy documented on MiscReader.
  fn capacity(&self, py: Python<'_>) -> PyResult<MiscCapacity> {
    PythonError::read(py, &self.owner, |owner| owner.misc().capacity()).map(Into::into)
  }

  /// Read hierarchical resource usage counts; root support starts with Linux 6.5.
  ///
  /// Performs fresh read-only I/O with the exception policy documented on MiscReader.
  fn current(&self, py: Python<'_>) -> PyResult<MiscCurrent> {
    PythonError::read(py, &self.owner, |owner| owner.misc().current()).map(Into::into)
  }

  /// Read historical hierarchical peak resource counts; since Linux 6.11.
  ///
  /// Performs fresh read-only I/O with the exception policy documented on MiscReader.
  fn peak(&self, py: Python<'_>) -> PyResult<MiscPeak> {
    PythonError::read(py, &self.owner, |owner| owner.misc().peak()).map(Into::into)
  }

  /// Read resource limits as MaxOr[int]; absent at the hierarchy root.
  ///
  /// Performs fresh read-only I/O with the exception policy documented on MiscReader.
  fn max(&self, py: Python<'_>) -> PyResult<MiscMax> {
    PythonError::read(py, &self.owner, |owner| owner.misc().max()).map(Into::into)
  }

  /// Read hierarchical resource max-boundary failure counts; since Linux 5.16.
  ///
  /// Performs fresh read-only I/O with the exception policy documented on MiscReader.
  fn events(&self, py: Python<'_>) -> PyResult<MiscEvents> {
    PythonError::read(py, &self.owner, |owner| owner.misc().events()).map(Into::into)
  }

  /// Read failures at this cgroup's limit or platform capacity; since Linux 6.11.
  ///
  /// Performs fresh read-only I/O with the exception policy documented on MiscReader.
  fn events_local(&self, py: Python<'_>) -> PyResult<MiscEvents> {
    PythonError::read(py, &self.owner, |owner| owner.misc().events_local()).map(Into::into)
  }
}

/// Machine-wide hardware-dependent resource capacities in resource counts.
///
/// Immutable snapshot keyed by kernel resource names; values can change on the
/// next read. Empty maps are valid. Omitted resources remain absent, distinct
/// from zero; inspecting resources performs no I/O.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MiscCapacity {
  resources: BTreeMap<String, u64>,
}

impl From<CoreCapacity> for MiscCapacity {
  fn from(value: CoreCapacity) -> Self {
    Self { resources: value.resources().iter().map(|(key, value)| (key.clone(), value.value)).collect() }
  }
}

#[pymethods]
impl MiscCapacity {
  /// Read-only resource-name mapping to integer resource counts in this stored snapshot.
  #[getter]
  fn resources(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, (&self.resources).into_py_dict(py)?.as_mapping()).unbind())
  }
}

/// Hierarchical resource usage counts; charged resources retain their original owner.
///
/// Immutable snapshot keyed by kernel resource names; values can change on the
/// next read. Empty maps are valid. Omitted resources remain absent, distinct
/// from zero; inspecting resources performs no I/O.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MiscCurrent {
  resources: BTreeMap<String, u64>,
}

impl From<CoreCurrent> for MiscCurrent {
  fn from(value: CoreCurrent) -> Self {
    Self { resources: value.resources().iter().map(|(key, value)| (key.clone(), value.value)).collect() }
  }
}

#[pymethods]
impl MiscCurrent {
  /// Read-only resource-name mapping to integer resource counts in this stored snapshot.
  #[getter]
  fn resources(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, (&self.resources).into_py_dict(py)?.as_mapping()).unbind())
  }
}

/// Historical hierarchical maximum resource counts; available since Linux 6.11.
///
/// Immutable snapshot keyed by kernel resource names; values can change on the
/// next read. Empty maps are valid. Omitted resources remain absent, distinct
/// from zero; inspecting resources performs no I/O.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MiscPeak {
  resources: BTreeMap<String, u64>,
}

impl From<CorePeak> for MiscPeak {
  fn from(value: CorePeak) -> Self {
    Self { resources: value.resources().iter().map(|(key, value)| (key.clone(), value.value)).collect() }
  }
}

#[pymethods]
impl MiscPeak {
  /// Read-only resource-name mapping to integer resource counts in this stored snapshot.
  #[getter]
  fn resources(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, (&self.resources).into_py_dict(py)?.as_mapping()).unbind())
  }
}

/// Cumulative resource max-boundary failures, with the scope of the selected file.
///
/// Hierarchical events aggregate limit-hit locations in this subtree. Local
/// events count failures at this cgroup's own limit or its platform-capacity
/// check, including descendant requests. Failures at an ancestor's limit or
/// capacity check are counted at that ancestor.
/// See https://github.com/torvalds/linux/blob/master/kernel/cgroup/misc.c.
///
/// Immutable snapshot keyed by kernel resource names; values can change on the
/// next read. Empty maps are valid. Omitted resources remain absent, distinct
/// from zero; inspecting resources performs no I/O.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MiscEvents {
  resources: BTreeMap<String, u64>,
}

impl From<CoreEvents> for MiscEvents {
  fn from(value: CoreEvents) -> Self {
    Self { resources: value.resources().iter().map(|(key, value)| (key.clone(), value.value)).collect() }
  }
}

#[pymethods]
impl MiscEvents {
  /// Read-only resource-name mapping to integer event counts in this stored snapshot.
  #[getter]
  fn resources(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    Ok(PyMappingProxy::new(py, (&self.resources).into_py_dict(py)?.as_mapping()).unbind())
  }
}

/// Immutable resource limits in counts; max means unlimited at this cgroup.
///
/// Ancestor limits and machine capacity still apply; configured limits may exceed
/// capacity. Zero is valid. Hardware-dependent resource names are preserved,
/// empty maps are valid, and this configuration can change between reads.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct MiscMax {
  resources: BTreeMap<String, MaxOr>,
}

impl From<CoreMax> for MiscMax {
  fn from(value: CoreMax) -> Self {
    Self {
      resources: value
        .resources()
        .iter()
        .map(|(name, value)| (name.clone(), MaxOr::from_value(*value, |value| PythonLimitValue::Integer(value.value))))
        .collect(),
    }
  }
}

#[pymethods]
impl MiscMax {
  /// Read-only resource-name mapping to immutable MaxOr[int] resource limits.
  ///
  /// Check is_max before value; value raises ValueError for an unlimited limit.
  #[getter]
  fn resources(&self, py: Python<'_>) -> PyResult<Py<PyMappingProxy>> {
    let dict = PyDict::new(py);
    for (name, value) in &self.resources {
      dict.set_item(name, Py::new(py, *value)?)?;
    }
    Ok(PyMappingProxy::new(py, dict.as_mapping()).unbind())
  }
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;

  use super::{
    super::tests::{check_frozen, check_limit, readonly},
    *,
  };

  #[test]
  fn converts_unsigned_counts_and_limits_to_immutable_snapshots() {
    Python::initialize();
    Python::attach(|py| {
      for (input, expected) in [
        ("sev 0", BTreeMap::from([("sev".to_owned(), 0_u64)])),
        ("sev 18446744073709551615", BTreeMap::from([("sev".to_owned(), u64::MAX)])),
        ("", BTreeMap::new()),
      ] {
        let objects = [
          assert_ok!(Py::new(py, MiscCurrent::from(assert_ok!(input.parse::<CoreCurrent>())))).into_any(),
          assert_ok!(Py::new(py, MiscCapacity::from(assert_ok!(input.parse::<CoreCapacity>())))).into_any(),
          assert_ok!(Py::new(py, MiscPeak::from(assert_ok!(input.parse::<CorePeak>())))).into_any(),
          assert_ok!(Py::new(
            py,
            MiscEvents::from(assert_ok!(input.replace("sev ", "sev.max ").parse::<CoreEvents>()))
          ))
          .into_any(),
        ];
        for object in objects {
          let values = readonly(object.bind(py), "resources", "sev");
          let dict = assert_ok!(py.get_type::<PyDict>().call1((&values,)));
          assert_eq!(assert_ok!(dict.extract::<BTreeMap<String, u64>>()), expected);
        }
      }
      let object = assert_ok!(Py::new(
        py,
        MiscMax::from(assert_ok!("sev max\nsev_es 18446744073709551615\nfuture 0".parse::<CoreMax>()))
      ));
      let values = readonly(object.bind(py).as_any(), "resources", "sev");
      for (key, expected) in [("sev", None), ("sev_es", Some(u64::MAX)), ("future", Some(0))] {
        let limit = assert_ok!(values.get_item(key));
        check_limit(&limit, expected);
        check_frozen(&limit, &["value"]);
      }
    });
  }
}
