//! Read-only Python bindings for individual v1 hierarchies.

use std::{path::PathBuf, sync::Arc};

use pyo3::prelude::*;
use sakai::{
  Bytes, Count, Time,
  v1::{Cgroup, core, cpu, cpuacct, memory, pids},
};

use super::{MaxOr, PythonError, handle::PythonPath};
use crate::quantity::Quantity;

/// An open, pinned cgroup v1 directory in one legacy hierarchy.
///
/// Open a handle for each controller of interest. Co-mounted controllers
/// share a directory. Each read fetches a fresh kernel value.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct V1Cgroup {
  owner: Arc<Cgroup>,
}

impl From<Cgroup> for V1Cgroup {
  fn from(owner: Cgroup) -> Self { Self { owner: Arc::new(owner) } }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl V1Cgroup {
  /// Open the current process's cgroup for a controller or named hierarchy.
  #[staticmethod]
  fn current(py: Python<'_>, controller: String) -> PyResult<Self> {
    PythonError::result(py, py.detach(move || Cgroup::from_current_process(&controller))).map(Into::into)
  }

  /// Open a process's cgroup using the caller's visible v1 mounts.
  #[staticmethod]
  fn from_pid(py: Python<'_>, pid: i64, controller: String) -> PyResult<Self> {
    let pid = u32::try_from(pid)?;
    PythonError::result(py, py.detach(move || Cgroup::from_pid(pid, &controller))).map(Into::into)
  }

  /// Open an explicit v1 directory path. Accepts str, bytes, and os.PathLike.
  #[staticmethod]
  fn from_path(
    py: Python<'_>,
    #[gen_stub(override_type(type_repr = "PathArg"))]
    #[pyo3(from_py_with = PythonPath::extract)]
    path: PathBuf,
  ) -> PyResult<Self> {
    PythonError::result(py, py.detach(move || Cgroup::from_path(&path))).map(Into::into)
  }

  /// Diagnostic path used when the handle was opened.
  #[getter]
  fn path(&self) -> PathBuf { self.owner.path().to_owned() }

  /// Open a child by name within the same hierarchy.
  fn child(
    &self,
    py: Python<'_>,
    #[gen_stub(override_type(type_repr = "PathArg"))]
    #[pyo3(from_py_with = PythonPath::extract)]
    name: PathBuf,
  ) -> PyResult<Self> {
    PythonError::read(py, &self.owner, move |group| group.child(name.as_os_str())).map(Into::into)
  }

  /// Return pinned handles for the currently visible children.
  fn children(&self, py: Python<'_>) -> PyResult<Vec<Self>> {
    PythonError::read(py, &self.owner, Cgroup::children).map(|children| children.into_iter().map(Into::into).collect())
  }

  /// Read the thread IDs in this cgroup in kernel order.
  fn tasks(&self, py: Python<'_>) -> PyResult<Vec<i32>> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<core::Tasks>)
      .map(|tasks| tasks.into_iter().map(|id| *id.as_ref()).collect())
  }

  /// Read the process IDs in this cgroup in kernel order.
  fn processes(&self, py: Python<'_>) -> PyResult<Vec<i32>> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<core::CgroupProcs>)
      .map(|processes| processes.into_iter().map(|id| *id.as_ref()).collect())
  }

  /// CPU bandwidth and accounting reads for this hierarchy.
  fn cpu(&self) -> V1CpuReader { V1CpuReader { owner: Arc::clone(&self.owner) } }

  /// Memory accounting and limit reads for this hierarchy.
  fn memory(&self) -> V1MemoryReader { V1MemoryReader { owner: Arc::clone(&self.owner) } }

  /// Task count and limit reads for this hierarchy.
  fn pids(&self) -> V1PidsReader { V1PidsReader { owner: Arc::clone(&self.owner) } }
}

/// Fresh CPU bandwidth and accounting reads in a v1 hierarchy.
///
/// Call bandwidth methods on a cpu hierarchy and usage on a cpuacct hierarchy.
/// Missing files raise InterfaceMissingError.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct V1CpuReader {
  owner: Arc<Cgroup>,
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl V1CpuReader {
  /// Read the local CPU quota as Time or an unlimited value.
  #[gen_stub(override_return_type(type_repr = "MaxOr[Time]"))]
  fn quota(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<cpu::CpuQuota>)
      .map(|snapshot| MaxOr::from_time(snapshot.value))
  }

  /// Read the length of the CPU bandwidth enforcement period.
  fn period(&self, py: Python<'_>) -> PyResult<Quantity<Time>> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<cpu::CpuPeriod>)
      .map(|snapshot| snapshot.value.as_ref().into())
  }

  /// Read CPU time charged to this group and its descendants.
  fn usage(&self, py: Python<'_>) -> PyResult<Quantity<Time>> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<cpuacct::CpuUsage>).map(|snapshot| snapshot.value.into())
  }

  /// Read this cgroup's relative fair-scheduler shares.
  fn shares(&self, py: Python<'_>) -> PyResult<u64> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<cpu::CpuShares>).map(|snapshot| snapshot.value)
  }
}

/// Fresh v1 memory accounting and limits. Missing files raise InterfaceMissingError.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct V1MemoryReader {
  owner: Arc<Cgroup>,
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl V1MemoryReader {
  /// Read approximate current memory usage in bytes.
  fn usage(&self, py: Python<'_>) -> PyResult<Quantity<Bytes>> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<memory::MemoryUsage>).map(|snapshot| snapshot.value.into())
  }

  /// Read the local memory limit in bytes, retaining the kernel's numeric unlimited sentinel.
  fn limit(&self, py: Python<'_>) -> PyResult<Quantity<Bytes>> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<memory::MemoryLimit>).map(|snapshot| snapshot.value.into())
  }

  /// Read peak memory usage in bytes.
  fn peak(&self, py: Python<'_>) -> PyResult<Quantity<Bytes>> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<memory::MemoryPeak>).map(|snapshot| snapshot.value.into())
  }

  /// Read approximate combined memory and swap usage in bytes.
  fn swap_usage(&self, py: Python<'_>) -> PyResult<Quantity<Bytes>> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<memory::MemorySwapUsage>)
      .map(|snapshot| snapshot.value.into())
  }
}

/// Fresh v1 task counts and limits, including individual threads.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct V1PidsReader {
  owner: Arc<Cgroup>,
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl V1PidsReader {
  /// Read the volatile hierarchical task count.
  fn current(&self, py: Python<'_>) -> PyResult<Quantity<Count>> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<pids::PidsCurrent>).map(|snapshot| snapshot.value.into())
  }

  /// Read the local task limit as MaxOr[Count].
  #[gen_stub(override_return_type(type_repr = "MaxOr[Count]"))]
  fn max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, Cgroup::read_value::<pids::PidsMax>)
      .map(|snapshot| MaxOr::from_count(snapshot.value))
  }
}
