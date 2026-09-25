use std::{borrow::Cow, io, path::PathBuf};

/// A cgroup discovery, read, or parse failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// An interface is unavailable (controller disabled, root, or older kernel).
  #[error("cgroup interface is missing: {path}")]
  FileMissing { path: PathBuf },
  /// The kernel does not support this operation for the cgroup.
  #[error("operation is not supported for this cgroup")]
  NotSupported,
  /// The directory is not on a cgroup v2 filesystem.
  #[error("not a cgroup v2 filesystem")]
  NotCgroupV2,
  /// Procfs reports a cgroup that has already been removed.
  #[error("cgroup has been deleted: {path}")]
  DeletedCgroup { path: PathBuf },
  /// Malformed interface contents, including the verbatim input in the detail.
  #[error("failed to parse {file}: {detail}")]
  Parse { file: &'static str, detail: String },
  /// Other operating-system errors, including permission failures.
  #[error(transparent)]
  Io(#[from] io::Error),
}

impl Error {
  #[cfg(target_os = "linux")]
  pub(crate) fn read(path: PathBuf, error: io::Error) -> Self {
    match error.kind() {
      | io::ErrorKind::NotFound => Self::FileMissing { path },
      | io::ErrorKind::Unsupported => Self::NotSupported,
      | _ => Self::Io(error),
    }
  }
}

/// An error parsing a cgroup interface file.
#[derive(Debug, thiserror::Error)]
pub enum ParseError<'a, E> {
  /// The file has a missing or excess field.
  #[error("cgroup content {raw:?} has {kind} field {field:?}")]
  Field {
    /// Whether the field is missing or excess.
    kind: FieldKind,
    /// Verbatim input passed to the parser.
    raw: Cow<'a, str>,
    /// Name of the missing field or position of the excess field.
    field: &'static str,
  },

  /// A field's value could not be parsed.
  #[error(
    "cgroup content {raw:?} has an invalid field {field:?} value {value:?}"
  )]
  Invalid {
    /// Verbatim input passed to the parser.
    raw: Cow<'a, str>,
    /// Name of the field whose value could not be parsed.
    field: &'static str,
    /// The value that failed to parse.
    value: Cow<'a, str>,
    /// The underlying parser error.
    #[source]
    source: E,
  },
}

impl<E> ParseError<'static, E> {
  /// Creates an error for a required field that is absent.
  #[must_use]
  pub fn missing(raw: &str, field: &'static str) -> Self {
    Self::Field {
      kind: FieldKind::Missing,
      raw: Cow::Owned(raw.into()),
      field,
    }
  }

  /// Creates an error for an unexpected field after the expected content.
  #[must_use]
  pub fn excess(raw: &str) -> Self {
    Self::Field {
      kind: FieldKind::Excess,
      raw: Cow::Owned(raw.into()),
      field: "additional",
    }
  }

  /// Creates an error for a field with an invalid value.
  #[must_use]
  pub fn invalid(
    raw: &str,
    field: &'static str,
    value: &str,
    source: E,
  ) -> Self {
    Self::Invalid {
      raw: Cow::Owned(raw.into()),
      field,
      value: Cow::Owned(value.into()),
      source,
    }
  }
}

/// Whether a cgroup interface file has too few or too many fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display)]
#[strum(serialize_all = "lowercase")]
pub enum FieldKind {
  /// An expected field is absent.
  Missing,
  /// An unexpected field is present.
  Excess,
}
