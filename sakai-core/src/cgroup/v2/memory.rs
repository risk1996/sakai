//! Memory controller interface files.

use std::str::FromStr;

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{ParseBytes, Parser},
  unit::Bytes,
};

/// A borrowed view of one open cgroup's memory interfaces.
///
/// Each method reads a fresh snapshot through the cgroup's directory. The
/// hierarchy root and cgroups without the memory controller can lack these
/// files; such reads return [`Error::FileMissing`]. Separate reads are not
/// atomic together.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Memory<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Memory<'_> {
  /// Live memory usage of this cgroup and its descendants, in bytes.
  pub fn current(&self) -> Result<MemoryCurrent, Error> {
    self.cgroup.parse("memory.current")
  }

  /// Configuration snapshot of this cgroup's hard memory limit in bytes.
  pub fn max(&self) -> Result<MemoryMax, Error> {
    self.cgroup.parse("memory.max")
  }

  /// Configuration snapshot of this cgroup's throttling limit in bytes.
  pub fn high(&self) -> Result<MemoryHigh, Error> {
    self.cgroup.parse("memory.high")
  }
}

/// Hierarchical memory usage reported by `memory.current`.
///
/// This is a point-in-time reading in bytes. It can temporarily exceed a
/// configured limit and should be read again for a fresh value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryCurrent {
  value: Bytes,
}

impl MemoryCurrent {
  /// Returns the current hierarchical usage in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes {
    self.value
  }
}

impl FromStr for MemoryCurrent {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("current")?,
      })
    })
  }
}

/// The hard memory limit configured by `memory.max`.
///
/// `max` means no limit is configured here; ancestor limits can still apply.
/// This is a point-in-time configuration reading in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryMax {
  value: MaxOr<Bytes>,
}

impl MemoryMax {
  /// Returns this cgroup's hard limit in bytes, or [`MaxOr::Max`].
  #[must_use]
  pub const fn value(self) -> MaxOr<Bytes> {
    self.value
  }
}

impl FromStr for MemoryMax {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("max")?,
      })
    })
  }
}

/// The memory throttling limit configured by `memory.high`.
///
/// Crossing this boundary triggers reclaim and throttling, not an OOM kill.
/// `max` means no limit is configured here; ancestor limits can still apply.
/// This is a point-in-time configuration reading in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryHigh {
  value: MaxOr<Bytes>,
}

impl MemoryHigh {
  /// Returns this cgroup's throttling limit in bytes, or [`MaxOr::Max`].
  #[must_use]
  pub const fn value(self) -> MaxOr<Bytes> {
    self.value
  }
}

impl FromStr for MemoryHigh {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("high")?,
      })
    })
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use uom::si::information::{byte, mebibyte};

  use super::*;

  #[test]
  fn parses_memory_current() {
    let cases = [
      (
        "0\n",
        Ok(MemoryCurrent {
          value: Bytes::new::<byte>(0),
        }),
      ),
      (
        " 1048576\n",
        Ok(MemoryCurrent {
          value: Bytes::new::<mebibyte>(1),
        }),
      ),
      (
        "18446744073709551615",
        Ok(MemoryCurrent {
          value: Bytes::new::<byte>(u64::MAX),
        }),
      ),
      ("", Err("cgroup content \"\" has missing field \"current\"")),
      (
        "1 2",
        Err("cgroup content \"1 2\" has excess field \"additional\""),
      ),
      (
        "max",
        Err(
          "cgroup content \"max\" has an invalid field \"current\" value \
           \"max\"",
        ),
      ),
      (
        "-1",
        Err(
          "cgroup content \"-1\" has an invalid field \"current\" value \"-1\"",
        ),
      ),
      (
        "18446744073709551616",
        Err(
          "cgroup content \"18446744073709551616\" has an invalid field \
           \"current\" value \"18446744073709551616\"",
        ),
      ),
    ];

    for (input, expected) in cases {
      match expected {
        | Ok(expected) => {
          assert_eq!(assert_ok!(input.parse::<MemoryCurrent>()), expected)
        },
        | Err(message) => assert_eq!(
          assert_err!(input.parse::<MemoryCurrent>()).to_string(),
          message
        ),
      }
    }
  }

  #[test]
  fn parses_memory_limits() {
    type ParseLimit =
      fn(&str) -> Result<MaxOr<Bytes>, ParseError<ParseValueError>>;

    let parsers: [(&str, ParseLimit); 2] = [
      ("max", |input| {
        input.parse::<MemoryMax>().map(MemoryMax::value)
      }),
      ("high", |input| {
        input.parse::<MemoryHigh>().map(MemoryHigh::value)
      }),
    ];
    let cases = [
      ("max\n", Ok(MaxOr::Max)),
      ("0", Ok(MaxOr::Value(Bytes::new::<byte>(0)))),
      ("1048576\n", Ok(MaxOr::Value(Bytes::new::<mebibyte>(1)))),
      (
        "18446744073709551615",
        Ok(MaxOr::Value(Bytes::new::<byte>(u64::MAX))),
      ),
      ("", Err("missing")),
      ("max 1", Err("excess")),
      ("-1", Err("invalid")),
      ("18446744073709551616", Err("invalid")),
      ("unlimited", Err("invalid")),
    ];

    for (input, expected) in cases {
      for (field, parse) in parsers {
        let actual = parse(input);
        match expected {
          | Ok(expected) => assert_eq!(
            assert_ok!(actual),
            expected,
            "field: {field}, input: {input:?}"
          ),
          | Err(kind) => {
            let error = assert_err!(actual);
            assert!(
              error.to_string().contains(kind),
              "field: {field}, input: {input:?}: {error}"
            );
            match error {
              | ParseError::Field { field: actual, .. } if kind == "missing" =>
              {
                assert_eq!(actual, field)
              },
              | ParseError::Field {
                field: "additional",
                ..
              } if kind == "excess" => {},
              | ParseError::Invalid { field: actual, .. }
                if kind == "invalid" =>
              {
                assert_eq!(actual, field)
              },
              | other => {
                panic!("unexpected error for {field}, {input:?}: {other}")
              },
            }
          },
        }
      }
    }
  }
}
