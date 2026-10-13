//! Read-only event delivery with independently owned kernel descriptors.

use std::{
  os::fd::{AsFd, AsRawFd, RawFd},
  sync::Mutex,
  time::Duration,
};

use pyo3::{
  exceptions::{PyRuntimeError, PyValueError},
  prelude::*,
};
use sakai::v2::notification::{
  ChangeNotification as CoreChangeNotification, ConfigurationWatcher as CoreConfigurationWatcher,
  EventPoller as CoreEventPoller,
};

use super::PythonError;

struct WaitTimeout;

impl WaitTimeout {
  fn parse(seconds: Option<f64>) -> PyResult<Option<Duration>> {
    seconds
      .map(|seconds| {
        Duration::try_from_secs_f64(seconds)
          .ok()
          .filter(|duration| i64::try_from(duration.as_secs()).is_ok())
          .ok_or_else(|| {
            PyValueError::new_err("timeout must be finite, nonnegative seconds within the supported duration range")
          })
      })
      .transpose()
  }
}

/// Read-only poll monitor for one cgroup event interface.
///
/// Owns its descriptor independently of the Cgroup object and closes it when
/// released. Notifications are coalesced, volatile hints to make a fresh read;
/// they do not contain counts or snapshots. PSI trigger files are excluded.
/// Only one wait may run at a time; concurrent waits raise RuntimeError.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct EventPoller {
  inner: Mutex<CoreEventPoller>,
  descriptor: RawFd,
}

impl From<CoreEventPoller> for EventPoller {
  fn from(inner: CoreEventPoller) -> Self { Self { descriptor: inner.as_fd().as_raw_fd(), inner: Mutex::new(inner) } }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl EventPoller {
  /// Wait for and acknowledge a kernel event notification, releasing the GIL.
  ///
  /// timeout is seconds: None waits indefinitely, zero checks immediately,
  /// and a finite nonnegative value bounds the kernel wait. Invalid values
  /// raise ValueError. Returns True for a change hint or False for timeout.
  /// After True, perform a fresh reader call; this wait discards file contents
  /// without parsing and does not change existing read semantics. A removed
  /// cgroup can fail during acknowledgement; failures raise OSError or SakaiError.
  /// Concurrent waits raise RuntimeError. See
  /// https://man7.org/linux/man-pages/man2/poll.2.html.
  #[pyo3(signature = (timeout=None))]
  fn wait(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<bool> {
    let timeout = WaitTimeout::parse(timeout)?;
    let result = py.detach(|| {
      let mut monitor = self
        .inner
        .try_lock()
        .map_err(|error| PyRuntimeError::new_err(format!("cannot acquire notification monitor: {error}")))?;
      Ok::<_, PyErr>(monitor.wait(timeout))
    })?;
    PythonError::result(py, result)
  }

  /// Borrow the descriptor number for external select.poll or epoll use.
  ///
  /// Register POLLPRI | POLLERR, never ordinary readable readiness: cgroup
  /// event files are always readable. Keep this object alive while registered
  /// and call wait(0) to acknowledge readiness before reading a fresh snapshot.
  /// Do not close, read, or transfer ownership of this borrowed descriptor.
  fn fileno(&self) -> RawFd { self.descriptor }
}

/// Inotify change monitor for one cgroup configuration interface.
///
/// Owns its descriptors independently of the Cgroup and closes them when
/// released. Events can coalesce and overflow; they contain no configuration
/// values. Read fresh values after modification and rescan after overflow.
/// Kernel-driven changes without fsnotify events may not be reported.
/// Cgroup interface removal need not emit inotify deletion events: a watch
/// can remain idle after removal. Use bounded waits and fresh lifecycle reads.
/// Only one wait may run at a time; concurrent waits raise RuntimeError.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct ConfigurationWatcher {
  inner: Mutex<CoreConfigurationWatcher>,
  descriptor: RawFd,
}

impl From<CoreConfigurationWatcher> for ConfigurationWatcher {
  fn from(inner: CoreConfigurationWatcher) -> Self {
    Self { descriptor: inner.as_fd().as_raw_fd(), inner: Mutex::new(inner) }
  }
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl ConfigurationWatcher {
  /// Wait for and consume queued configuration hints, releasing the GIL.
  ///
  /// timeout is seconds: None waits indefinitely, zero checks immediately,
  /// and a finite nonnegative value bounds the kernel wait. Invalid values
  /// raise ValueError. Returns an immutable ChangeNotification, or None for
  /// timeout. Flags can overlap; no flag includes a fresh configuration value.
  /// A reported removal ends the watch; recreate it for a replacement file/cgroup.
  /// Interface disappearance need not notify; an indefinite wait can remain
  /// blocked after deletion. Use bounded waits and fresh reads to check lifecycle.
  /// Subsequent waits raise BrokenPipeError. The watch follows inode renames.
  /// I/O failures raise OSError or SakaiError; concurrent waits raise RuntimeError.
  /// See https://man7.org/linux/man-pages/man7/inotify.7.html.
  #[pyo3(signature = (timeout=None))]
  fn wait(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<Option<ChangeNotification>> {
    let timeout = WaitTimeout::parse(timeout)?;
    let result = py.detach(|| {
      let mut monitor = self
        .inner
        .try_lock()
        .map_err(|error| PyRuntimeError::new_err(format!("cannot acquire notification monitor: {error}")))?;
      Ok::<_, PyErr>(monitor.wait(timeout))
    })?;
    PythonError::result(py, result).map(|notification| notification.map(Into::into))
  }

  /// Borrow the inotify descriptor for select, selectors, poll, or epoll.
  ///
  /// Register ordinary readable readiness (POLLIN), keep this object alive,
  /// and call wait(0) to consume queued events. Do not close, read, or transfer
  /// ownership of this borrowed descriptor; its readiness carries no values.
  fn fileno(&self) -> RawFd { self.descriptor }
}

/// Immutable flags from a consumed batch of inotify notifications.
///
/// These flags are volatile hints, not configuration snapshots or event counts.
/// Multiple flags may be True, and repeated changes may be coalesced.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChangeNotification {
  /// Whether a write or kernel content notification was reported; read fresh values.
  modified: bool,
  /// Whether the kernel reported removal, unmount, or watch invalidation.
  /// False does not establish that the cgroup interface still exists.
  removed: bool,
  /// Whether the inotify queue overflowed; events were lost, so rescan values.
  overflowed: bool,
}

impl From<CoreChangeNotification> for ChangeNotification {
  fn from(value: CoreChangeNotification) -> Self {
    Self { modified: value.modified(), removed: value.removed(), overflowed: value.overflowed() }
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;

  #[test]
  fn validates_timeout_seconds() {
    assert_eq!(assert_ok!(WaitTimeout::parse(None)), None);
    for (seconds, duration) in [(0.0, Duration::ZERO), (0.25, Duration::from_millis(250))] {
      assert_eq!(assert_ok!(WaitTimeout::parse(Some(seconds))), Some(duration));
    }
    for invalid in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e19, f64::MAX] {
      assert_err!(WaitTimeout::parse(Some(invalid)));
    }
  }
}
