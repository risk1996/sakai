//! Memory controller interface files.

use std::str::FromStr;

pub use stat::*;

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;
#[cfg(target_os = "linux")]
use crate::pressure::Pressure;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{ParseBytes, Parser},
  unit::Bytes,
};

mod stat;

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

  /// Live peak usage, in bytes, since cgroup creation for this fresh descriptor.
  pub fn peak(&self) -> Result<MemoryPeak, Error> {
    self.cgroup.parse("memory.peak")
  }

  /// Live breakdown of memory usage, page quantities, and event counts.
  pub fn stat(&self) -> Result<MemoryStat, Error> {
    self.cgroup.parse("memory.stat")
  }

  /// Live PSI averages and totals. Never registers a pressure trigger.
  pub fn pressure(&self) -> Result<Pressure, Error> {
    self.cgroup.parse("memory.pressure")
  }

  /// Configuration snapshot of this cgroup's hard memory limit in bytes.
  pub fn max(&self) -> Result<MemoryMax, Error> {
    self.cgroup.parse("memory.max")
  }

  /// Configuration snapshot of this cgroup's throttling limit in bytes.
  pub fn high(&self) -> Result<MemoryHigh, Error> {
    self.cgroup.parse("memory.high")
  }

  /// Configuration snapshot of this cgroup's best-effort memory protection.
  pub fn low(&self) -> Result<MemoryLow, Error> {
    self.cgroup.parse("memory.low")
  }

  /// Configuration snapshot of this cgroup's hard memory protection.
  pub fn min(&self) -> Result<MemoryMin, Error> {
    self.cgroup.parse("memory.min")
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

  /// Parses one decimal byte count, rejecting missing or excess fields.
  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("current")?,
      })
    })
  }
}

/// Peak hierarchical memory usage reported by `memory.peak`.
///
/// The kernel can reset the peak by writing to an open file descriptor. This
/// reader opens a fresh read-only descriptor for each call, so it never resets
/// the value and observes the peak since cgroup creation. Older kernels may
/// lack this file; the reader then returns [`Error::FileMissing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryPeak {
  value: Bytes,
}

impl MemoryPeak {
  /// Returns the peak hierarchical usage in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes {
    self.value
  }
}

impl FromStr for MemoryPeak {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("peak")?,
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

  /// Parses one decimal byte limit or the literal `max`.
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

  /// Parses one decimal byte limit or the literal `max`.
  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("high")?,
      })
    })
  }
}

/// The best-effort memory protection configured by `memory.low`.
///
/// Reclaim avoids memory below the effective low boundary while unprotected
/// memory is available. Ancestor settings can reduce the effective protection.
/// This is a point-in-time configuration reading in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryLow {
  value: Bytes,
}

impl MemoryLow {
  /// Returns this cgroup's configured best-effort protection in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes {
    self.value
  }
}

impl FromStr for MemoryLow {
  type Err = ParseError<ParseValueError>;

  /// Parses one decimal byte count.
  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("low")?,
      })
    })
  }
}

/// The hard memory protection configured by `memory.min`.
///
/// Memory below the effective min boundary cannot be reclaimed. If no
/// unprotected reclaimable memory remains, an OOM kill can follow. Ancestor
/// settings can reduce the effective protection. This is a point-in-time
/// configuration reading in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryMin {
  value: Bytes,
}

impl MemoryMin {
  /// Returns this cgroup's configured hard protection in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes {
    self.value
  }
}

impl FromStr for MemoryMin {
  type Err = ParseError<ParseValueError>;

  /// Parses one decimal byte count.
  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("min")?,
      })
    })
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use indoc::indoc;
  use uom::si::{
    information::{byte, mebibyte},
    ratio::percent,
    time::microsecond,
  };

  use super::*;
  use crate::pressure::Pressure;

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
  fn parses_memory_peak() {
    for (input, expected) in [
      (
        "0\n",
        Ok(MemoryPeak {
          value: Bytes::new::<byte>(0),
        }),
      ),
      (
        "1048576\n",
        Ok(MemoryPeak {
          value: Bytes::new::<mebibyte>(1),
        }),
      ),
      (
        "18446744073709551615",
        Ok(MemoryPeak {
          value: Bytes::new::<byte>(u64::MAX),
        }),
      ),
      ("", Err("missing field \"peak\"")),
      ("1 2", Err("excess field \"additional\"")),
      ("max", Err("invalid field \"peak\"")),
      ("-1", Err("invalid field \"peak\"")),
      ("18446744073709551616", Err("invalid field \"peak\"")),
    ] {
      match expected {
        | Ok(expected) => {
          assert_eq!(assert_ok!(input.parse::<MemoryPeak>()), expected)
        },
        | Err(message) => assert!(
          assert_err!(input.parse::<MemoryPeak>())
            .to_string()
            .contains(message),
          "input: {input:?}"
        ),
      }
    }
  }

  #[test]
  fn parses_memory_pressure_with_shared_psi_type() {
    let pressure = assert_ok!(
      indoc! {"
      some avg10=2.50 avg60=1.25 avg300=0.50 total=12345
      full avg10=0.25 avg60=0.10 avg300=0.00 total=678
    "}
      .parse::<Pressure>()
    );
    assert_eq!(
      pressure.some().avg10(),
      crate::unit::Ratio::new::<percent>(2.50)
    );
    assert_eq!(
      pressure.some().total(),
      crate::unit::Time::new::<microsecond>(12_345)
    );
    assert_eq!(
      pressure.full().map(|full| full.total()),
      Some(crate::unit::Time::new::<microsecond>(678))
    );
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

  #[test]
  fn parses_memory_protections() {
    type ParseProtection =
      fn(&str) -> Result<Bytes, ParseError<ParseValueError>>;

    let parsers: [(&str, ParseProtection); 2] = [
      ("low", |input| {
        input.parse::<MemoryLow>().map(MemoryLow::value)
      }),
      ("min", |input| {
        input.parse::<MemoryMin>().map(MemoryMin::value)
      }),
    ];
    let cases = [
      ("0\n", Ok(Bytes::new::<byte>(0))),
      ("1048576\n", Ok(Bytes::new::<mebibyte>(1))),
      ("18446744073709551615", Ok(Bytes::new::<byte>(u64::MAX))),
      ("", Err("missing")),
      ("1 2", Err("excess")),
      ("max", Err("invalid")),
      ("-1", Err("invalid")),
      ("18446744073709551616", Err("invalid")),
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
