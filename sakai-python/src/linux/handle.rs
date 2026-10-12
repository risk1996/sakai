//! Handle bindings and conversions.

use std::{path::PathBuf, sync::Arc};

use pyo3::{exceptions::PyValueError, prelude::*};

use super::{
  ConfigurationWatcher, CoreReader, CpuReader, CpusetReader, EventPoller, IoReader, MemoryReader, PidsReader,
  PythonError, dmem::DmemReader, hugetlb::HugetlbReader, misc::MiscReader, rdma::RdmaReader,
};

pub(super) struct PythonPath;

impl PythonPath {
  fn extract<T>(object: &Bound<'_, T>) -> PyResult<PathBuf> {
    // fsdecode first applies os.fspath and decodes bytes with surrogateescape.
    // PyO3 0.29's PathBuf extractor accepts only str results from os.fspath.
    object.py().import("os")?.call_method1("fsdecode", (object.as_any(),))?.extract()
  }
}

/// An open, pinned cgroup v2 directory handle.
///
/// Readers retain this handle's ownership, so they remain usable after the
/// Python Cgroup object is released. Each read fetches a fresh kernel value;
/// separate calls do not form an atomic snapshot.
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct Cgroup {
  pub(super) owner: Arc<sakai::Cgroup>,
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

  /// Return a reader for CPU and NUMA memory-node placement.
  fn cpuset(&self) -> CpusetReader { CpusetReader { owner: Arc::clone(&self.owner) } }

  /// Return a reader for memory controller interfaces.
  fn memory(&self) -> MemoryReader { MemoryReader { owner: Arc::clone(&self.owner) } }

  /// Return a reader for per-device I/O statistics, pressure, and configuration.
  fn io(&self) -> IoReader { IoReader { owner: Arc::clone(&self.owner) } }

  /// Return a pinned read-only reader for the hugetlb controller.
  fn hugetlb(&self) -> HugetlbReader { HugetlbReader { owner: Arc::clone(&self.owner) } }

  /// Return a pinned read-only reader for the rdma controller.
  fn rdma(&self) -> RdmaReader { RdmaReader { owner: Arc::clone(&self.owner) } }

  /// Return a pinned read-only reader for the dmem controller.
  fn dmem(&self) -> DmemReader { DmemReader { owner: Arc::clone(&self.owner) } }

  /// Return a pinned read-only reader for the misc controller.
  fn misc(&self) -> MiscReader { MiscReader { owner: Arc::clone(&self.owner) } }

  /// Return a process-number reader; kernel PID counts include threads.
  fn pids(&self) -> PidsReader { PidsReader { owner: Arc::clone(&self.owner) } }

  /// Return a reader for core cgroup topology interfaces.
  fn core(&self) -> CoreReader { CoreReader { owner: Arc::clone(&self.owner) } }

  /// Read interrupt pressure as a full-only PSI snapshot.
  ///
  /// Requires Linux 6.1, CONFIG_PSI, and CONFIG_IRQ_TIME_ACCOUNTING. Missing interfaces raise
  /// InterfaceMissingError; other failures follow the reader exception policy.
  /// See https://docs.kernel.org/accounting/psi.html.
  fn irq_pressure(&self, py: Python<'_>) -> PyResult<super::pressure::IrqPressure> {
    PythonError::read(py, &self.owner, sakai::Cgroup::irq_pressure).map(Into::into)
  }

  /// Open a read-only poll monitor for a supported event interface.
  ///
  /// Accepts cgroup.events, memory.events, memory.events.local,
  /// memory.swap.events, pids.events, and cpuset.cpus.partition. Unknown names raise ValueError;
  /// unavailable interfaces raise InterfaceMissingError with their path.
  /// The monitor owns its descriptor after this Cgroup is released. It
  /// acknowledges the initial state when opened; notifications are coalesced
  /// hints to perform a fresh reader call, not counter or state snapshots.
  /// PSI pressure files are excluded because installing a trigger requires
  /// a write. See https://docs.kernel.org/admin-guide/cgroup-v2.html.
  fn poll_events(&self, py: Python<'_>, interface: &str) -> PyResult<EventPoller> {
    let interface = interface.parse().map_err(|error| PyValueError::new_err(format!("{error}")))?;
    PythonError::read(py, &self.owner, move |owner| owner.poll_events(interface)).map(Into::into)
  }

  /// Open an inotify monitor for a supported configuration interface.
  ///
  /// Accepts configuration filenames documented in the package README,
  /// never arbitrary paths or usage/statistic files. Unknown names raise
  /// ValueError; missing files raise InterfaceMissingError with their path.
  /// The monitor owns its descriptors after this Cgroup is released.
  /// Changes are coalesced hints: read the configuration again after a
  /// notification, and rescan after queue overflow. Reported removal ends the
  /// watch. Interface disappearance may go unreported; use bounded waits and
  /// fresh reads to check lifecycle. Registration requires /proc/self/fd access.
  /// This operation never writes configuration or installs PSI triggers.
  /// See https://man7.org/linux/man-pages/man7/inotify.7.html.
  fn watch_configuration(&self, py: Python<'_>, interface: &str) -> PyResult<ConfigurationWatcher> {
    let interface = interface.parse().map_err(|error| PyValueError::new_err(format!("{error}")))?;
    PythonError::read(py, &self.owner, move |owner| owner.watch_configuration(interface)).map(Into::into)
  }

  fn __repr__(&self) -> String { format!("Cgroup({:?})", self.owner.path()) }
}

#[cfg(test)]
mod tests {
  use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
  };

  use assertables::assert_ok;
  use pyo3::types::PyBytes;
  use sakai::Error;

  use super::*;

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
