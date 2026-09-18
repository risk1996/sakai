use std::{borrow::Cow, num::ParseIntError, str::SplitAsciiWhitespace};

use uom::si::time::nanosecond;

use crate::cgroup::common::{
  error::{FieldKind, ParseError},
  unit::{MaxOr, Time},
};

/// A zero-sized marker for cgroup values encoded in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParseMicroseconds;

/// A numeric field is malformed or outside the target type's supported range.
#[derive(Debug, thiserror::Error)]
pub enum ParseValueError {
  /// The field is not a valid integer in the input storage type.
  #[error(transparent)]
  Integer(#[from] ParseIntError),
  /// The value exceeds a supported range, including after unit conversion.
  #[error("value is outside the supported range")]
  OutOfRange,
}

/// Iterator over fields from one cgroup interface file.
pub struct Parser<'a> {
  raw: &'a str,
  fields: SplitAsciiWhitespace<'a>,
}

impl<'a> Parser<'a> {
  /// Creates a parser that records `raw` in any parse error.
  #[must_use]
  pub fn new(raw: &'a str) -> Self {
    Self {
      raw,
      fields: raw.split_ascii_whitespace(),
    }
  }

  /// Parses the next field as `T` using the `Unit` marker.
  pub fn next_field<Unit, T>(
    &mut self,
    field: &'static str,
  ) -> Result<T, ParseError<'static, T::Error>>
  where
    T: ParseCgroup<Unit>, {
    let value = self.fields.next().ok_or_else(|| ParseError::Field {
      kind: FieldKind::Missing,
      raw: Cow::Owned(self.raw.into()),
      field,
    })?;

    T::parse_cgroup(value).map_err(|source| ParseError::Invalid {
      raw: Cow::Owned(self.raw.into()),
      field,
      value: Cow::Owned(value.into()),
      source,
    })
  }

  /// Returns an error if an excess field remains.
  pub fn finish<E>(&mut self) -> Result<(), ParseError<'static, E>> {
    if self.fields.next().is_some() {
      return Err(ParseError::Field {
        kind: FieldKind::Excess,
        raw: Cow::Owned(self.raw.into()),
        field: "additional",
      });
    }

    Ok(())
  }
}

/// Parses a cgroup field using a type-level encoding marker.
pub trait ParseCgroup<Unit>: Sized {
  /// The underlying error produced when parsing this value.
  type Error;

  /// Parses a cgroup field.
  fn parse_cgroup(value: &str) -> Result<Self, Self::Error>;
}

impl ParseCgroup<ParseMicroseconds> for Time {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    let nanoseconds = value
      .parse::<u64>()?
      .checked_mul(1_000)
      .ok_or(ParseValueError::OutOfRange)?;
    Ok(Self::new::<nanosecond>(nanoseconds))
  }
}

impl<Unit, T> ParseCgroup<Unit> for MaxOr<T>
where
  T: ParseCgroup<Unit>,
{
  type Error = T::Error;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    match value {
      | "max" => Ok(Self::Max),
      | value => T::parse_cgroup(value).map(Self::Value),
    }
  }
}

#[cfg(test)]
mod tests {
  use assertables::assert_err;

  use super::*;

  #[test]
  fn rejects_times_that_overflow_nanosecond_storage() {
    for input in ["18446744073709552", "18446744073709551615"] {
      let mut parser = Parser::new(input);
      let error =
        assert_err!(parser.next_field::<ParseMicroseconds, Time>("duration"));
      match error {
        | ParseError::Invalid {
          raw,
          field,
          value,
          source: ParseValueError::OutOfRange,
        } => {
          assert_eq!(raw, input);
          assert_eq!(field, "duration");
          assert_eq!(value, input);
        },
        | other => {
          panic!("expected conversion overflow for {input:?}, got {other:?}")
        },
      }
    }
  }
}
