//! Errors bindings and conversions.

use std::{io, path::PathBuf, sync::Arc};

#[cfg(not(feature = "stubs"))]
use pyo3::create_exception;
use pyo3::{
  PyTypeInfo,
  exceptions::{PyException, PyOSError},
  prelude::*,
};
#[cfg(feature = "stubs")]
use pyo3_stub_gen::create_exception;
use sakai::Error;

create_exception!(_sakai, SakaiError, PyException, "Base error for Sakai cgroup operations.");
create_exception!(
  _sakai,
  InterfaceMissingError,
  SakaiError,
  "A cgroup interface file is missing; path identifies the interface."
);
create_exception!(_sakai, NotCgroupV2Error, SakaiError, "The requested path is not on a cgroup v2 filesystem.");
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
  "The interface read is unsupported; path and filename identify it, and errno retains the OS error code."
);

pub(super) struct PythonError;

impl PythonError {
  fn with_path<E: PyTypeInfo>(py: Python<'_>, message: String, path: PathBuf) -> PyErr {
    let error = PyErr::new::<E, _>(message);
    // A path is diagnostic. PyO3 uses os.fspath-compatible, lossless path conversion.
    if let Err(attribute_error) = error.value(py).setattr("path", path) {
      return attribute_error;
    }
    error
  }

  pub(super) fn from_core(py: Python<'_>, error: Error) -> PyErr {
    match error {
      | Error::FileMissing { path } => {
        Self::with_path::<InterfaceMissingError>(py, format!("cgroup interface is missing: {}", path.display()), path)
      },
      | Error::NotSupported { path, source } => {
        let error = Self::with_path::<NotSupportedError>(
          py,
          format!("reading {} is not supported: {source}", path.display()),
          path.clone(),
        );
        match error
          .value(py)
          .setattr("errno", source.raw_os_error())
          .and_then(|()| error.value(py).setattr("filename", path))
        {
          | Ok(()) => error,
          | Err(error) => error,
        }
      },
      | Error::NotCgroupV2 => NotCgroupV2Error::new_err("not a cgroup v2 filesystem"),
      | Error::DeletedCgroup { path } => {
        Self::with_path::<DeletedCgroupError>(py, format!("cgroup has been deleted: {}", path.display()), path)
      },
      | Error::Parse { path, source } => {
        Self::with_path::<CgroupParseError>(py, format!("failed to parse {}: {source}", path.display()), path)
      },
      // Construct OSError first so errno selects its standard subclass even
      // on Python 3.11, where a lazy PyErr can retain OSError as its type.
      | Error::Io(source) => Self::os_error(py, source, None),
      | Error::Operation { operation, path, source } => {
        let error = Self::os_error(py, source, Some(path.clone()));
        match error.value(py).setattr("path", path).and_then(|()| error.value(py).setattr("operation", operation)) {
          | Ok(()) => error,
          | Err(error) => error,
        }
      },
      // The core error enum can grow without breaking downstream matches.
      | other => SakaiError::new_err(other.to_string()),
    }
  }

  fn os_error(py: Python<'_>, source: io::Error, path: Option<PathBuf>) -> PyErr {
    // Passing filename to OSError preserves it and lets errno choose subclasses.
    // Paths use PyO3's lossless conversion, including surrogate escapes.
    match py.get_type::<PyOSError>().call1((source.raw_os_error(), source.to_string(), path)) {
      | Ok(exception) => PyErr::from_value(exception),
      | Err(error) => error,
    }
  }

  pub(super) fn result<T>(py: Python<'_>, result: Result<T, Error>) -> PyResult<T> {
    result.map_err(|error| Self::from_core(py, error))
  }

  pub(super) fn read<T: Send + 'static>(
    py: Python<'_>,
    owner: &Arc<sakai::Cgroup>,
    read: impl FnOnce(&sakai::Cgroup) -> Result<T, Error> + Send + 'static,
  ) -> PyResult<T> {
    // Detach only after owning all inputs; shared handles never depend on the GIL.
    // Construct Python values and translate exceptions after reattaching.
    let owner = Arc::clone(owner);
    Self::result(py, py.detach(move || read(&owner)))
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use pyo3::exceptions::{PyFileNotFoundError, PyPermissionError};

  use super::*;

  #[test]
  fn preserves_python_error_categories_paths_and_errno() {
    Python::initialize();
    Python::attach(|py| {
      let path = PathBuf::from("/sys/fs/cgroup/example");
      for (error, exception_type, expected_path) in [
        (Error::FileMissing { path: path.clone() }, py.get_type::<InterfaceMissingError>(), Some(&path)),
        (Error::DeletedCgroup { path: path.clone() }, py.get_type::<DeletedCgroupError>(), Some(&path)),
        (
          Error::Parse { path: path.clone(), source: Box::new(io::Error::other("invalid quota")) },
          py.get_type::<CgroupParseError>(),
          Some(&path),
        ),
        (Error::NotCgroupV2, py.get_type::<NotCgroupV2Error>(), None),
        (
          Error::NotSupported { path: path.clone(), source: io::Error::from_raw_os_error(95) },
          py.get_type::<NotSupportedError>(),
          Some(&path),
        ),
      ] {
        let translated = assert_err!(PythonError::result::<()>(py, Err(error)));
        assert!(translated.is_instance(py, &exception_type));
        assert!(translated.is_instance_of::<SakaiError>(py));
        assert!(!translated.is_instance_of::<PyOSError>(py));
        match expected_path {
          | Some(expected) => {
            let actual = assert_ok!(assert_ok!(translated.value(py).getattr("path")).extract::<PathBuf>());
            assert_eq!(&actual, expected);
          },
          | None => assert!(!assert_ok!(translated.value(py).hasattr("path"))),
        }
        if translated.is_instance_of::<CgroupParseError>(py) {
          assert!(translated.to_string().contains("invalid quota"));
        }
        if translated.is_instance_of::<NotSupportedError>(py) {
          assert_eq!(assert_ok!(assert_ok!(translated.value(py).getattr("errno")).extract::<i32>()), 95);
          assert_eq!(assert_ok!(assert_ok!(translated.value(py).getattr("filename")).extract::<PathBuf>()), path);
        }
      }
      for (errno, exception_type) in
        [(2, py.get_type::<PyFileNotFoundError>()), (13, py.get_type::<PyPermissionError>())]
      {
        let translated = PythonError::from_core(py, Error::Io(io::Error::from_raw_os_error(errno)));
        assert!(translated.is_instance(py, &exception_type));
        assert!(!translated.is_instance_of::<SakaiError>(py));
        assert_eq!(assert_ok!(assert_ok!(translated.value(py).getattr("errno")).extract::<i32>()), errno);
        let translated = PythonError::from_core(py, Error::Operation {
          operation: "read procfs",
          path: path.clone(),
          source: io::Error::from_raw_os_error(errno),
        });
        assert!(translated.is_instance(py, &exception_type));
        assert_eq!(assert_ok!(assert_ok!(translated.value(py).getattr("errno")).extract::<i32>()), errno);
        for name in ["path", "filename"] {
          assert_eq!(assert_ok!(assert_ok!(translated.value(py).getattr(name)).extract::<PathBuf>()), path);
        }
        assert_eq!(
          assert_ok!(assert_ok!(translated.value(py).getattr("operation")).extract::<String>()),
          "read procfs"
        );
      }
    });
  }
}
