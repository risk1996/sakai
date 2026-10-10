use std::{fmt::Debug, marker::PhantomData, str::FromStr};

use assertables::{assert_err, assert_ok};
use uom::si::information::byte;

use super::*;
use crate::error::FieldKind;

/// Parser cases assert values and structured errors without repeating diagnostics.
pub(crate) struct Cases<T>(PhantomData<T>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure<'a> {
  Missing(&'a str),
  Excess,
  Invalid(&'a str, &'a str),
}

impl Failure<'_> {
  fn check(self, input: &str, error: ParseError<ParseValueError>) {
    let (raw, actual) = match &error {
      | ParseError::Field { raw, field, kind: FieldKind::Missing } => (raw, Failure::Missing(field)),
      | ParseError::Field { raw, field, kind: FieldKind::Excess } => {
        assert_eq!(*field, "additional", "input: {input:?}");
        (raw, Failure::Excess)
      },
      | ParseError::Invalid { raw, field, value, .. } => (raw, Failure::Invalid(field, value)),
    };
    assert_eq!(raw.as_ref(), input);
    assert_eq!(actual, self, "input: {input:?}: {error}");
  }
}

impl<T> Cases<T>
where
  T: FromStr<Err = ParseError<ParseValueError>> + Debug + PartialEq,
{
  pub(crate) fn check<'a>(cases: impl IntoIterator<Item = (&'a str, Result<T, Failure<'a>>)>) {
    for (input, expected) in cases {
      let actual = input.parse::<T>();
      match expected {
        | Ok(expected) => {
          assert_eq!(assert_ok!(actual), expected, "input: {input:?}")
        },
        | Err(expected) => expected.check(input, assert_err!(actual)),
      }
    }
  }

  pub(crate) fn bytes(field: &'static str, new: impl Fn(Bytes) -> T) {
    Self::check([
      ("0\n", Ok(new(Bytes::new::<byte>(0)))),
      ("1048576", Ok(new(Bytes::new::<byte>(1_048_576)))),
      ("1048576\n", Ok(new(Bytes::new::<byte>(1_048_576)))),
      (" 1048576\n", Ok(new(Bytes::new::<byte>(1_048_576)))),
      ("18446744073709551615", Ok(new(Bytes::new::<byte>(u64::MAX)))),
      ("", Err(Failure::Missing(field))),
      ("1 2", Err(Failure::Excess)),
      ("max", Err(Failure::Invalid(field, "max"))),
      ("-1", Err(Failure::Invalid(field, "-1"))),
      ("18446744073709551616", Err(Failure::Invalid(field, "18446744073709551616"))),
    ]);
  }

  pub(crate) fn limit(field: &'static str, new: impl Fn(MaxOr<Bytes>) -> T) {
    Self::check([
      ("max\n", Ok(new(MaxOr::Max))),
      ("0", Ok(new(MaxOr::Value(Bytes::new::<byte>(0))))),
      ("1048576", Ok(new(MaxOr::Value(Bytes::new::<byte>(1_048_576))))),
      ("1048576\n", Ok(new(MaxOr::Value(Bytes::new::<byte>(1_048_576))))),
      ("18446744073709551615", Ok(new(MaxOr::Value(Bytes::new::<byte>(u64::MAX))))),
      ("", Err(Failure::Missing(field))),
      ("max 1", Err(Failure::Excess)),
      ("-1", Err(Failure::Invalid(field, "-1"))),
      ("18446744073709551616", Err(Failure::Invalid(field, "18446744073709551616"))),
      ("unlimited", Err(Failure::Invalid(field, "unlimited"))),
    ]);
  }

  pub(crate) fn boolean(field: &'static str, new: impl Fn(bool) -> T) {
    Self::check([
      ("0\n", Ok(new(false))),
      ("1", Ok(new(true))),
      ("", Err(Failure::Missing(field))),
      ("1 0", Err(Failure::Excess)),
      ("2", Err(Failure::Invalid(field, "2"))),
      ("-1", Err(Failure::Invalid(field, "-1"))),
      ("true", Err(Failure::Invalid(field, "true"))),
    ]);
  }
}

#[test]
fn displays_parse_errors() {
  for (error, expected) in [
    (ParseError::missing(" \n", "value"), "cgroup content \" \\n\" has missing field \"value\""),
    (ParseError::excess("1 2\n"), "cgroup content \"1 2\\n\" has excess field \"additional\""),
    (
      ParseError::invalid("nope\n", "value", "nope", ParseValueError::OutOfRange),
      "cgroup content \"nope\\n\" has an invalid field \"value\" value \"nope\"",
    ),
  ] {
    assert_eq!(error.to_string(), expected);
  }
}

#[test]
fn rejects_times_that_overflow_nanosecond_storage() {
  for input in ["18446744073709552", "18446744073709551615"] {
    let error =
      assert_err!(Parser::parse(input, |parser| { parser.next_field::<ParseMicroseconds, Time>("duration") }));
    match error {
      | ParseError::Invalid { raw, field, value, source: ParseValueError::OutOfRange } => {
        assert_eq!(raw.as_ref(), input);
        assert_eq!(field, "duration");
        assert_eq!(value.as_ref(), input);
      },
      | other => {
        panic!("expected conversion overflow for {input:?}, got {other:?}")
      },
    }
  }
}
