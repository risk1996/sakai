use std::fmt::Write;

use uom::si::time::nanosecond;

use crate::{
  Bytes, MaxOr, NonZeroTime, Time,
  error::{EncodeError, ParseValueError},
  parse::{EncodeCgroup, ParseBytes, ParseCgroup, ParseNonZeroMicroseconds},
};

pub(super) struct Nanoseconds;

impl ParseCgroup<Nanoseconds> for Time {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(Self::new::<nanosecond>(value.parse()?)) }
}

impl EncodeCgroup<Nanoseconds> for Time {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{}", self.get::<nanosecond>()).ok() }
}

pub(super) struct Unsigned;

impl ParseCgroup<Unsigned> for u64 {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(value.parse()?) }
}

impl EncodeCgroup<Unsigned> for u64 {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{self}").ok() }
}

impl ParseCgroup<Unsigned> for u32 {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> { Ok(value.parse()?) }
}

impl EncodeCgroup<Unsigned> for u32 {
  fn encode_token(&self, output: &mut String) -> Option<()> { write!(output, "{self}").ok() }
}

pub(super) struct CpuQuotaToken;

impl ParseCgroup<CpuQuotaToken> for MaxOr<NonZeroTime> {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    match value {
      | "-1" => Ok(Self::Max),
      | value => <NonZeroTime as ParseCgroup<ParseNonZeroMicroseconds>>::parse_cgroup(value).map(Self::Value),
    }
  }
}

impl EncodeCgroup<CpuQuotaToken> for MaxOr<NonZeroTime> {
  fn encode_token(&self, output: &mut String) -> Option<()> {
    match self {
      | Self::Max => {
        output.push_str("-1");
        Some(())
      },
      | Self::Value(value) => EncodeCgroup::<ParseNonZeroMicroseconds>::encode_token(value, output),
    }
  }
}

pub(super) fn encode_byte_limit(limit: MaxOr<Bytes>, field: &'static str) -> Result<String, EncodeError> {
  match limit {
    | MaxOr::Max => Ok("-1".into()),
    | MaxOr::Value(value) => {
      let mut output = String::new();
      EncodeCgroup::<ParseBytes>::encode_field(&value, &mut output, field)?;
      Ok(output)
    },
  }
}
