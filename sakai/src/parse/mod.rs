use std::{collections::BTreeMap, fmt::Write, str::SplitAsciiWhitespace};

use nutype::nutype;
use uom::si::{
  frequency::hertz, information::byte, information_rate::byte_per_second, ratio::percent, time::nanosecond,
};

use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  unit::{Bytes, BytesPerSecond, Count, NonZeroTime, OperationsPerSecond, Pages, Ratio, Time},
};

mod codec;
pub(crate) use codec::{EncodeCgroup, cgroup_value, encode_pair};

const MAX_TOKEN: &str = "max";

/// Borrowed keyed values with the complete source file for field errors.
pub(crate) struct KeyedFields<'a> {
  raw: &'a str,
  values: BTreeMap<&'a str, &'a str>,
}

impl<'a> KeyedFields<'a> {
  pub(crate) fn new<E>(
    raw: &'a str,
    fields: impl IntoIterator<Item = Result<(&'a str, &'a str), E>>,
  ) -> Result<Self, E> {
    let values = fields.into_iter().collect::<Result<_, _>>()?;
    Ok(Self { raw, values })
  }

  /// Reads checked key/value lines, preserving each interface's field names.
  pub(crate) fn parse<E>(raw: &'a str, value_field: impl Fn(&str) -> &'static str) -> Result<Self, ParseError<E>> {
    Self::new(
      raw,
      raw.lines().filter(|line| !line.trim().is_empty()).map(|line| {
        Parser::parse_line(raw, line, |parser| {
          let key = parser.next_raw_field("key")?;
          Ok((key, parser.next_raw_field(value_field(key))?))
        })
      }),
    )
  }

  pub(crate) fn contains_any(&self, fields: impl IntoIterator<Item = impl Into<&'static str>>) -> bool {
    fields.into_iter().any(|field| self.values.contains_key(field.into()))
  }

  pub(crate) fn iter(&self) -> impl Iterator<Item = (&'a str, &'a str)> + '_ {
    self.values.iter().map(|(&key, &value)| (key, value))
  }

  /// Parses named values using one interface's units and diagnostic label.
  pub(crate) fn parse_values<Unit, T>(&self, field: &'static str) -> Result<BTreeMap<String, T>, ParseError<T::Error>>
  where
    T: ParseCgroup<Unit>, {
    self.iter().map(|(key, value)| Ok((key.to_owned(), T::parse_field(self.raw, field, value)?))).collect()
  }

  pub(crate) fn required<Unit, T>(&self, field: impl Into<&'static str>) -> Result<T, ParseError<T::Error>>
  where
    T: ParseCgroup<Unit>, {
    let field = field.into();
    let value = self.values.get(field).ok_or_else(|| ParseError::missing(self.raw, field))?;
    T::parse_field(self.raw, field, value)
  }

  pub(crate) fn optional<Unit, T>(&self, field: impl Into<&'static str>) -> Result<Option<T>, ParseError<T::Error>>
  where
    T: ParseCgroup<Unit>, {
    let field = field.into();
    self.values.get(field).map(|value| T::parse_field(self.raw, field, value)).transpose()
  }
}

/// Parser for fields from one cgroup interface file.
pub(crate) struct Parser<'a> {
  raw: &'a str,
  fields: SplitAsciiWhitespace<'a>,
}

impl<'a> Parser<'a> {
  /// Parses exactly one typed value with its diagnostic field name.
  pub(crate) fn single<Unit, T>(raw: &'a str, field: &'static str) -> Result<T, ParseError<T::Error>>
  where
    T: ParseCgroup<Unit>, {
    Self::parse(raw, |parser| parser.next_field::<Unit, T>(field))
  }

  /// Parses a value, rejecting any fields left after the closure succeeds.
  pub(crate) fn parse<T, E>(
    raw: &'a str,
    f: impl FnOnce(&mut Self) -> Result<T, ParseError<E>>,
  ) -> Result<T, ParseError<E>> {
    Self::parse_line(raw, raw, f)
  }

  /// Parses one line while recording the complete file in parse errors.
  pub(crate) fn parse_line<T, E>(
    raw: &'a str,
    line: &'a str,
    f: impl FnOnce(&mut Self) -> Result<T, ParseError<E>>,
  ) -> Result<T, ParseError<E>> {
    let mut parser = Self::new(raw, line);
    let value = f(&mut parser)?;
    parser.finish()?;
    Ok(value)
  }

  /// Creates a parser that records `raw` in any parse error.
  #[must_use]
  fn new(raw: &'a str, fields: &'a str) -> Self { Self { raw, fields: fields.split_ascii_whitespace() } }

  /// Returns the next field without interpreting its value.
  pub(crate) fn next_raw_field<E>(&mut self, field: &'static str) -> Result<&'a str, ParseError<E>> {
    self.fields.next().ok_or_else(|| ParseError::missing(self.raw, field))
  }

  /// Parses the next field as `T` using the `Unit` marker.
  pub(crate) fn next_field<Unit, T>(&mut self, field: &'static str) -> Result<T, ParseError<T::Error>>
  where
    T: ParseCgroup<Unit>, {
    T::parse_field(self.raw, field, self.next_raw_field(field)?)
  }

  /// Returns an error if an excess field remains.
  fn finish<E>(&mut self) -> Result<(), ParseError<E>> {
    match self.fields.next() {
      | Some(..) => Err(ParseError::excess(self.raw)),
      | None => Ok(()),
    }
  }
}

/// Parses a cgroup field using a type-level encoding marker.
pub(crate) trait ParseCgroup<Unit>: Sized {
  /// The underlying error produced when parsing this value.
  type Error;

  /// Parses a cgroup field.
  fn parse_cgroup(value: &str) -> Result<Self, Self::Error>;

  /// Parses a cgroup field and attaches its source context to an error.
  fn parse_field(raw: &str, field: &'static str, value: &str) -> Result<Self, ParseError<Self::Error>> {
    Self::parse_cgroup(value).map_err(|source| ParseError::invalid(raw, field, value, source))
  }
}

/// A zero-sized marker for cgroup booleans encoded as `0` or `1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ParseBoolean;

impl ParseCgroup<ParseBoolean> for bool {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    match value.parse::<u8>()? {
      | 0 => Ok(false),
      | 1 => Ok(true),
      | _ => Err(ParseValueError::OutOfRange),
    }
  }
}

impl EncodeCgroup<ParseBoolean> for bool {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", u8::from(*self)).ok() }
}

/// A zero-sized marker for cgroup event counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ParseCount;

impl ParseCgroup<ParseCount> for Count {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self { value: value.parse()?, ..Default::default() }) }
}

impl EncodeCgroup<ParseCount> for Count {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", self.value).ok() }
}

// Some kernels print these unsigned 32-bit counters with a signed format.
pub(crate) struct ParseBandwidthCount;

impl EncodeCgroup<ParseBandwidthCount> for Count {
  fn encode_token(&self, output: &mut String) -> Option<()> { EncodeCgroup::<ParseCount>::encode_token(self, output) }
}

impl ParseCgroup<ParseBandwidthCount> for Count {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    let value = if value.starts_with('-') {
      u64::from(u32::from_ne_bytes(value.parse::<i32>()?.to_ne_bytes()))
    } else {
      value.parse()?
    };
    Ok(Self { value, ..Default::default() })
  }
}

/// A zero-sized marker for cgroup page quantities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ParsePages;

impl ParseCgroup<ParsePages> for Pages {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self { value: value.parse()?, ..Default::default() }) }
}

impl EncodeCgroup<ParsePages> for Pages {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", self.value).ok() }
}

/// A zero-sized marker for cgroup memory amounts encoded in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ParseBytes;

impl ParseCgroup<ParseBytes> for Bytes {
  type Error = ParseValueError;

  /// Parses a decimal byte count without unit conversion.
  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self::new::<byte>(value.parse()?)) }
}

impl EncodeCgroup<ParseBytes> for Bytes {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", self.get::<byte>()).ok() }
}

/// A cgroup percentage encoded as a decimal from 0 through 100.
#[nutype(validate(finite, greater_or_equal = 0.0, less_or_equal = 100.0), derive(Debug, Clone, Copy, PartialEq))]
pub(crate) struct Percent(f64);

impl ParseCgroup<Percent> for Ratio {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    let percent_value = Percent::try_new(value.parse::<f64>()?).map_err(|_error| ParseValueError::OutOfRange)?;

    Ok(Self::new::<percent>(percent_value.into_inner()))
  }
}

impl EncodeCgroup<Percent> for Ratio {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    encode_percent(self, output, |value| Percent::try_new(value).is_ok())
  }
}

pub(crate) fn encode_percent(value: &Ratio, output: &mut String, valid: impl Fn(f64) -> bool) -> Option<()> {
  let candidate = value.get::<percent>();
  // Unit conversion rounds. Try neighboring decimal values and check the decoded bits.
  let exact = [candidate, candidate.next_down(), candidate.next_up()]
    .into_iter()
    .find(|&candidate| valid(candidate) && Ratio::new::<percent>(candidate).value.to_bits() == value.value.to_bits())?;
  write!(output, "{exact}").ok()
}

pub(crate) fn encode_scaled_time(value: &Time, output: &mut String, scale: u64) -> Option<()> {
  let nanoseconds = value.get::<nanosecond>();
  if nanoseconds.checked_rem(scale)? != 0 {
    return None;
  }
  write!(output, "{}", nanoseconds.checked_div(scale)?).ok()
}

/// A zero-sized marker for cgroup values encoded in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ParseMicroseconds;

impl ParseMicroseconds {
  const NANOSECONDS_PER_MICROSECOND: u64 = 1_000;
}

impl ParseCgroup<ParseMicroseconds> for Time {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    let nanoseconds = value
      .parse::<u64>()?
      .checked_mul(ParseMicroseconds::NANOSECONDS_PER_MICROSECOND)
      .ok_or(ParseValueError::OutOfRange)?;
    Ok(Self::new::<nanosecond>(nanoseconds))
  }
}

impl EncodeCgroup<ParseMicroseconds> for Time {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    encode_scaled_time(self, output, ParseMicroseconds::NANOSECONDS_PER_MICROSECOND)
  }
}

/// A zero-sized marker for nonzero cgroup values encoded in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ParseNonZeroMicroseconds;

impl ParseCgroup<ParseNonZeroMicroseconds> for NonZeroTime {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    let time = <Time as ParseCgroup<ParseMicroseconds>>::parse_cgroup(value)?;
    Self::try_new(time).map_err(|_error| ParseValueError::OutOfRange)
  }
}

impl EncodeCgroup<ParseNonZeroMicroseconds> for NonZeroTime {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    EncodeCgroup::<ParseMicroseconds>::encode_token(self.as_ref(), output)
  }
}

impl<Unit, T> ParseCgroup<Unit> for MaxOr<T>
where
  T: ParseCgroup<Unit>,
{
  type Error = T::Error;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    match value {
      | MAX_TOKEN => Ok(Self::Max),
      | value => T::parse_cgroup(value).map(Self::Value),
    }
  }
}

impl<Unit, T: EncodeCgroup<Unit>> EncodeCgroup<Unit> for MaxOr<T> {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    match self {
      | Self::Max => {
        output.push_str(MAX_TOKEN);
        Some(())
      },
      | Self::Value(value) => value.encode_token(output),
    }
  }
}

pub(crate) struct ParseBandwidth;

impl ParseCgroup<ParseBandwidth> for BytesPerSecond {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self::new::<byte_per_second>(value.parse()?)) }
}

pub(crate) struct ParseIops;

impl ParseCgroup<ParseIops> for OperationsPerSecond {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self::new::<hertz>(value.parse()?)) }
}

impl EncodeCgroup<ParseBandwidth> for BytesPerSecond {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", self.get::<byte_per_second>()).ok() }
}

impl EncodeCgroup<ParseIops> for OperationsPerSecond {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", self.get::<hertz>()).ok() }
}

#[cfg(test)]
pub(crate) mod tests;
