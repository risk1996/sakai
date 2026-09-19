use std::borrow::Cow;

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
