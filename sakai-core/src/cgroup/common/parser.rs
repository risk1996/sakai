use std::{num::ParseIntError, str::SplitAsciiWhitespace};

use uom::si::time::nanosecond;

use crate::cgroup::common::{
  error::ParseError,
  unit::{Count, MaxOr, NonZeroTime, Time},
};

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

/// Parser for fields from one cgroup interface file.
pub struct Parser<'a> {
  raw: &'a str,
  fields: SplitAsciiWhitespace<'a>,
}

impl<'a> Parser<'a> {
  /// Parses a value, rejecting any fields left after the closure succeeds.
  pub fn parse<T, E>(
    raw: &'a str,
    f: impl FnOnce(&mut Self) -> Result<T, ParseError<'static, E>>,
  ) -> Result<T, ParseError<'static, E>> {
    Self::parse_line(raw, raw, f)
  }

  /// Parses one line while recording the complete file in parse errors.
  pub fn parse_line<T, E>(
    raw: &'a str,
    line: &'a str,
    f: impl FnOnce(&mut Self) -> Result<T, ParseError<'static, E>>,
  ) -> Result<T, ParseError<'static, E>> {
    let mut parser = Self::new(raw, line);
    let value = f(&mut parser)?;
    parser.finish()?;
    Ok(value)
  }

  /// Creates a parser that records `raw` in any parse error.
  #[must_use]
  fn new(raw: &'a str, fields: &'a str) -> Self {
    Self {
      raw,
      fields: fields.split_ascii_whitespace(),
    }
  }

  /// Returns the next field without interpreting its value.
  pub fn next_raw_field<E>(
    &mut self,
    field: &'static str,
  ) -> Result<&'a str, ParseError<'static, E>> {
    self
      .fields
      .next()
      .ok_or_else(|| ParseError::missing(self.raw, field))
  }

  /// Parses the next field as `T` using the `Unit` marker.
  pub fn next_field<Unit, T>(
    &mut self,
    field: &'static str,
  ) -> Result<T, ParseError<'static, T::Error>>
  where
    T: ParseCgroup<Unit>, {
    let value = self.next_raw_field(field)?;

    T::parse_field(self.raw, field, value)
  }

  /// Returns an error if an excess field remains.
  fn finish<E>(&mut self) -> Result<(), ParseError<'static, E>> {
    match self.fields.next() {
      | Some(..) => Err(ParseError::excess(self.raw)),
      | None => Ok(()),
    }
  }
}

/// Parses a cgroup field using a type-level encoding marker.
pub trait ParseCgroup<Unit>: Sized {
  /// The underlying error produced when parsing this value.
  type Error;

  /// Parses a cgroup field.
  fn parse_cgroup(value: &str) -> Result<Self, Self::Error>;

  /// Parses a cgroup field and attaches its source context to an error.
  fn parse_field(
    raw: &str,
    field: &'static str,
    value: &str,
  ) -> Result<Self, ParseError<'static, Self::Error>> {
    Self::parse_cgroup(value)
      .map_err(|source| ParseError::invalid(raw, field, value, source))
  }
}

/// A zero-sized marker for cgroup event counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParseCount;

impl ParseCgroup<ParseCount> for Count {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    Ok(Self::new(value.parse()?))
  }
}

/// A zero-sized marker for cgroup values encoded in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParseMicroseconds;

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

/// A zero-sized marker for nonzero cgroup values encoded in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParseNonZeroMicroseconds;

impl ParseCgroup<ParseNonZeroMicroseconds> for NonZeroTime {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    let time = <Time as ParseCgroup<ParseMicroseconds>>::parse_cgroup(value)?;
    Self::try_new(time).map_err(|_error| ParseValueError::OutOfRange)
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
      let error = assert_err!(Parser::parse(input, |parser| {
        parser.next_field::<ParseMicroseconds, Time>("duration")
      }));
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
