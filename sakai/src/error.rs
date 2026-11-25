use std::{
  io,
  num::{ParseFloatError, ParseIntError},
  path::PathBuf,
};

/// A cgroup discovery, read, write, or codec failure.
///
/// Downstream matches must handle future error categories.
///
/// ```compile_fail,E0004
/// use sakai::Error;
/// match Error::NotCgroupV2 {
///     Error::FileMissing { .. } | Error::NotSupported { .. } | Error::NotCgroupV2
///     | Error::DeletedCgroup { .. } | Error::Parse { .. } | Error::Encode { .. }
///     | Error::Operation { .. } | Error::Io(_) => (),
/// }
/// ```
///
/// See [Linux cgroup file formats and availability](https://docs.kernel.org/admin-guide/cgroup-v2.html#interface-files).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
  /// An interface is unavailable (controller disabled, root, or older kernel).
  #[error("cgroup interface is missing: {path}")]
  FileMissing {
    /// Diagnostic path of the missing cgroup interface file.
    path: PathBuf,
  },
  /// The kernel does not support this operation for the cgroup.
  #[error("cgroup operation on {} is not supported: {source}", path.display())]
  NotSupported {
    /// Diagnostic path of the unsupported interface.
    path: PathBuf,
    /// The original operating-system error.
    #[source]
    source: io::Error,
  },
  /// The directory is not on a cgroup v2 filesystem.
  #[error("not a cgroup v2 filesystem")]
  NotCgroupV2,
  /// The directory is outside the selected cgroup v1 filesystem.
  #[error("not on the requested cgroup v1 filesystem")]
  NotCgroupV1,
  /// The process has no membership for the requested v1 controller or named hierarchy.
  #[error("the process has no cgroup v1 membership for {controller}")]
  V1MembershipMissing {
    /// Controller or named hierarchy requested by the caller.
    controller: String,
  },
  /// No visible v1 mount exposes the requested membership.
  #[error("no cgroup v1 mount exposes the process's membership for {controller}")]
  V1MountMissing {
    /// Controller or named hierarchy requested by the caller.
    controller: String,
  },
  /// Procfs reports a cgroup that has already been removed.
  #[error("cgroup has been deleted: {path}")]
  DeletedCgroup {
    /// Cgroup membership path reported as deleted by procfs.
    path: PathBuf,
  },
  /// Malformed interface contents at the given interface path.
  #[error("failed to parse {}: {source}", path.display())]
  Parse {
    /// Full path to the interface.
    path: PathBuf,
    /// The original parser error, including raw content when provided.
    #[source]
    source: ParseError<ParseValueError>,
  },
  /// A command could not be encoded for its interface.
  #[error("failed to encode {}: {source}", path.display())]
  Encode {
    /// Full path to the interface.
    path: PathBuf,
    /// The original encoding error.
    #[source]
    source: EncodeError,
  },
  /// An operating-system failure with operation and diagnostic path context.
  #[error("failed to {operation} {}: {source}", path.display())]
  Operation {
    /// The operation that failed, such as reading an interface or procfs.
    operation: &'static str,
    /// Diagnostic path of the file or directory involved.
    path: PathBuf,
    /// The original operating-system error, retaining its native errno.
    #[source]
    source: io::Error,
  },
  /// Other failures without a known path, including invalid caller inputs.
  #[error(transparent)]
  Io(#[from] io::Error),
}

impl Error {
  #[cfg(target_os = "linux")]
  pub(crate) fn read(path: PathBuf, error: io::Error) -> Self {
    Self::interface_operation("read interface", path, error)
  }

  #[cfg(target_os = "linux")]
  pub(crate) fn interface_operation(operation: &'static str, path: PathBuf, error: io::Error) -> Self {
    match error.kind() {
      | io::ErrorKind::NotFound => Self::FileMissing { path },
      | io::ErrorKind::Unsupported => Self::NotSupported { path, source: error },
      | _ => Self::Operation { operation, path, source: error },
    }
  }

  #[cfg(target_os = "linux")]
  pub(crate) fn procfs(error: procfs::ProcError) -> Self {
    // Procfs normalizes permission and missing-file errors, discarding their
    // original errno. Reconstruct their standard errno; preserve native errno
    // for Io, and retain the source for non-OS failures.
    let (source, path) = match error {
      | procfs::ProcError::Io(source, path) => (source, path),
      | procfs::ProcError::PermissionDenied(path) => (rustix::io::Errno::ACCESS.into(), path),
      | procfs::ProcError::NotFound(path) => (rustix::io::Errno::NOENT.into(), path),
      | procfs::ProcError::Incomplete(ref path) => {
        let path = path.clone();
        (io::Error::other(error), path)
      },
      | other => (io::Error::other(other), None),
    };
    match path {
      | Some(path) => Self::Operation { operation: "read procfs", path, source },
      | None => Self::Io(source),
    }
  }
}

/// An error parsing a cgroup interface file.
///
/// See [Linux cgroup file formats and availability](https://docs.kernel.org/admin-guide/cgroup-v2.html#interface-files).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError<E> {
  /// The file has a missing or excess field.
  #[error("cgroup content {raw:?} has {kind} field {field:?}")]
  Field {
    /// Whether the field is missing or excess.
    kind: FieldKind,
    /// Verbatim input passed to the parser.
    raw: Box<str>,
    /// Name of the missing field or position of the excess field.
    field: &'static str,
  },

  /// A field's value could not be parsed.
  #[error("cgroup content {raw:?} has an invalid field {field:?} value {value:?}")]
  Invalid {
    /// Verbatim input passed to the parser.
    raw: Box<str>,
    /// Name of the field whose value could not be parsed.
    field: &'static str,
    /// The value that failed to parse.
    value: Box<str>,
    /// The underlying parser error.
    #[source]
    source: E,
  },
}

impl<E> ParseError<E> {
  /// Creates an error for a required field that is absent.
  #[must_use]
  pub(crate) fn missing(raw: &str, field: &'static str) -> Self {
    Self::Field { kind: FieldKind::Missing, raw: raw.into(), field }
  }

  /// Creates an error for an unexpected field after the expected content.
  #[must_use]
  pub(crate) fn excess(raw: &str) -> Self {
    Self::Field { kind: FieldKind::Excess, raw: raw.into(), field: "additional" }
  }

  /// Creates an error for a field with an invalid value.
  #[must_use]
  pub(crate) fn invalid(raw: &str, field: &'static str, value: &str, source: E) -> Self {
    Self::Invalid { raw: raw.into(), field, value: value.into(), source }
  }
}

/// A field cannot be represented exactly in its cgroup text format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("field {field} cannot be represented exactly in cgroup text")]
#[non_exhaustive]
pub struct EncodeError {
  pub(crate) field: &'static str,
}

impl EncodeError {
  /// Returns the name of the field that could not be encoded.
  #[must_use]
  pub const fn field(&self) -> &'static str { self.field }
}

/// A field value is malformed, unrecognized, or outside its supported range.
///
/// See [Linux cgroup file formats and availability](https://docs.kernel.org/admin-guide/cgroup-v2.html#interface-files).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseValueError {
  /// The field is not a valid integer in the input storage type.
  #[error(transparent)]
  Integer(#[from] ParseIntError),
  /// The field is not a valid floating-point number.
  #[error(transparent)]
  Float(#[from] ParseFloatError),
  /// The field is not a recognized value of its enum.
  #[error(transparent)]
  UnknownVariant(#[from] strum::ParseError),
  /// The value exceeds a supported range, including after unit conversion.
  #[error("value is outside the supported range")]
  OutOfRange,
}

/// Whether a cgroup interface file has too few or too many fields.
///
/// See [Linux cgroup file formats and availability](https://docs.kernel.org/admin-guide/cgroup-v2.html#interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display)]
#[strum(serialize_all = "lowercase")]
#[non_exhaustive]
pub enum FieldKind {
  /// An expected field is absent.
  Missing,
  /// An unexpected field is present.
  Excess,
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
  use std::error::Error as _;

  use assertables::assert_some;

  use super::*;

  #[test]
  fn interface_errors_retain_categories_paths_and_sources() {
    let path = PathBuf::from("/sys/fs/cgroup/example/memory.current");
    assert!(
      matches!(Error::read(path.clone(), rustix::io::Errno::NOENT.into()), Error::FileMissing { path: actual } if actual == path)
    );
    for (errno, unsupported) in [(rustix::io::Errno::ACCESS, false), (rustix::io::Errno::OPNOTSUPP, true)] {
      let error = Error::read(path.clone(), errno.into());
      let source = assert_some!(match &error {
        | Error::NotSupported { path: actual, source } => {
          assert!(unsupported);
          assert_eq!(actual, &path);
          Some(source)
        },
        | Error::Operation { operation, path: actual, source } => {
          assert!(!unsupported);
          assert_eq!(*operation, "read interface");
          assert_eq!(actual, &path);
          Some(source)
        },
        | _ => None,
      });
      assert_eq!(source.raw_os_error(), Some(errno.raw_os_error()));
      assert_eq!(
        error.source().and_then(|source| source.downcast_ref::<io::Error>()).and_then(io::Error::raw_os_error),
        source.raw_os_error()
      );
    }
  }

  #[test]
  fn procfs_errors_retain_paths_and_available_errno() {
    let path = PathBuf::from("/proc/123/cgroup");
    for (input, expected) in [
      (procfs::ProcError::Io(rustix::io::Errno::IO.into(), Some(path.clone())), rustix::io::Errno::IO),
      (procfs::ProcError::PermissionDenied(Some(path.clone())), rustix::io::Errno::ACCESS),
      (procfs::ProcError::NotFound(Some(path.clone())), rustix::io::Errno::NOENT),
    ] {
      let error = Error::procfs(input);
      assert!(
        matches!(&error, Error::Operation { operation: "read procfs", path: actual, source } if actual == &path && source.raw_os_error() == Some(expected.raw_os_error()))
      );
      assert!(error.source().and_then(|source| source.downcast_ref::<io::Error>()).is_some());
    }
    let error = Error::procfs(procfs::ProcError::Incomplete(Some(path.clone())));
    assert!(
      matches!(&error, Error::Operation { path: actual, source, .. } if actual == &path && source.get_ref().and_then(|source| source.downcast_ref::<procfs::ProcError>()).is_some())
    );
  }
}
