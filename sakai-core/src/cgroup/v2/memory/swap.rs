//! Swap accounting and limits in the memory controller.

use std::str::FromStr;

#[cfg(target_os = "linux")]
use super::super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{KeyedFields, ParseBytes, ParseCount, Parser},
  unit::{Bytes, Count},
};

/// A borrowed view of one cgroup's swap interfaces.
///
/// Each method reads a fresh snapshot. Root cgroups and kernels without a
/// given interface return [`Error::FileMissing`].
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Swap<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Swap<'_> {
  /// Current swap usage of this cgroup and its descendants, in bytes.
  pub fn current(&self) -> Result<SwapCurrent, Error> {
    self.cgroup.parse("memory.swap.current")
  }

  /// Peak swap usage since cgroup creation for this fresh descriptor.
  pub fn peak(&self) -> Result<SwapPeak, Error> {
    self.cgroup.parse("memory.swap.peak")
  }

  /// Hard swap limit configured for this cgroup.
  pub fn max(&self) -> Result<SwapMax, Error> {
    self.cgroup.parse("memory.swap.max")
  }

  /// Swap throttling limit configured for this cgroup.
  pub fn high(&self) -> Result<SwapHigh, Error> {
    self.cgroup.parse("memory.swap.high")
  }

  /// Swap high, max, and allocation failure counters.
  pub fn events(&self) -> Result<SwapEvents, Error> {
    self.cgroup.parse("memory.swap.events")
  }
}

/// Hierarchical swap usage reported by `memory.swap.current`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwapCurrent {
  value: Bytes,
}

impl SwapCurrent {
  /// Returns the current swap usage in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes {
    self.value
  }
}

impl FromStr for SwapCurrent {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("current")?,
      })
    })
  }
}

/// Peak hierarchical swap usage reported by `memory.swap.peak`.
///
/// A write to an open file descriptor resets the peak for that descriptor.
/// This reader opens a fresh read-only descriptor, so its value is the peak
/// since cgroup creation. Older kernels may lack this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwapPeak {
  value: Bytes,
}

impl SwapPeak {
  /// Returns the peak swap usage in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes {
    self.value
  }
}

impl FromStr for SwapPeak {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("peak")?,
      })
    })
  }
}

/// Hard swap limit reported by `memory.swap.max`.
///
/// `max` means no limit is configured here; ancestor limits can still apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwapMax {
  value: MaxOr<Bytes>,
}

impl SwapMax {
  /// Returns the hard swap limit in bytes, or [`MaxOr::Max`].
  #[must_use]
  pub const fn value(self) -> MaxOr<Bytes> {
    self.value
  }
}

impl FromStr for SwapMax {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("max")?,
      })
    })
  }
}

/// Swap throttling limit reported by `memory.swap.high`.
///
/// Exceeding this limit throttles further allocations. It is intended for
/// userspace out-of-memory handling rather than routine swap control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwapHigh {
  value: MaxOr<Bytes>,
}

impl SwapHigh {
  /// Returns the swap throttling limit in bytes, or [`MaxOr::Max`].
  #[must_use]
  pub const fn value(self) -> MaxOr<Bytes> {
    self.value
  }
}

impl FromStr for SwapHigh {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("high")?,
      })
    })
  }
}

/// Swap event counters reported by `memory.swap.events`.
///
/// The `high` counter is absent on older kernels. Unknown future keys are
/// ignored; known counters must contain valid unsigned event counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwapEvents {
  high: Option<Count>,
  max: Count,
  fail: Count,
}

impl SwapEvents {
  /// Times swap usage exceeded the high threshold, when reported.
  #[must_use]
  pub const fn high(self) -> Option<Count> {
    self.high
  }

  /// Times a swap allocation failed at the max boundary.
  #[must_use]
  pub const fn max(self) -> Count {
    self.max
  }

  /// Swap allocation failures from the limit or system-wide exhaustion.
  #[must_use]
  pub const fn fail(self) -> Count {
    self.fail
  }
}

impl FromStr for SwapEvents {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let values = contents
      .lines()
      .filter(|line| !line.trim().is_empty())
      .map(|line| {
        Parser::parse_line(contents, line, |parser| {
          let key = parser.next_raw_field("key")?;
          let field = match key {
            | "high" => "high",
            | "max" => "max",
            | "fail" => "fail",
            | _ => "value",
          };
          Ok((key, parser.next_raw_field(field)?))
        })
      })
      .collect::<Result<Vec<_>, _>>()?;
    let fields = KeyedFields::new(contents, values);

    Ok(Self {
      high: fields.optional::<ParseCount, _>("high")?,
      max: fields.required::<ParseCount, _>("max")?,
      fail: fields.required::<ParseCount, _>("fail")?,
    })
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use indoc::indoc;
  use uom::si::information::{byte, mebibyte};

  use super::*;

  #[test]
  fn parses_swap_usage() {
    type ParseUsage = fn(&str) -> Result<Bytes, ParseError<ParseValueError>>;
    let parsers: [(&str, ParseUsage); 2] = [
      ("current", |input| {
        input.parse::<SwapCurrent>().map(SwapCurrent::value)
      }),
      ("peak", |input| {
        input.parse::<SwapPeak>().map(SwapPeak::value)
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

    for (field, parse) in parsers {
      for (input, expected) in cases {
        match expected {
          | Ok(expected) => assert_eq!(assert_ok!(parse(input)), expected),
          | Err(kind) => {
            let error = assert_err!(parse(input));
            assert!(error.to_string().contains(kind), "{field}: {error}");
          },
        }
      }
    }
  }

  #[test]
  fn parses_swap_limits() {
    type ParseLimit =
      fn(&str) -> Result<MaxOr<Bytes>, ParseError<ParseValueError>>;
    let parsers: [(&str, ParseLimit); 2] = [
      ("max", |input| input.parse::<SwapMax>().map(SwapMax::value)),
      ("high", |input| {
        input.parse::<SwapHigh>().map(SwapHigh::value)
      }),
    ];
    let cases = [
      ("max\n", Ok(MaxOr::Max)),
      ("0", Ok(MaxOr::Value(Bytes::new::<byte>(0)))),
      ("1048576", Ok(MaxOr::Value(Bytes::new::<mebibyte>(1)))),
      ("", Err("missing")),
      ("max 1", Err("excess")),
      ("-1", Err("invalid")),
      ("18446744073709551616", Err("invalid")),
    ];

    for (field, parse) in parsers {
      for (input, expected) in cases {
        match expected {
          | Ok(expected) => assert_eq!(assert_ok!(parse(input)), expected),
          | Err(kind) => {
            let error = assert_err!(parse(input));
            assert!(error.to_string().contains(kind), "{field}: {error}");
          },
        }
      }
    }
  }

  #[test]
  fn parses_swap_events() {
    let cases = [
      (
        indoc! {"
          high 2
          max 3
          fail 4
        "},
        Ok(SwapEvents {
          high: Some(Count {
            value: 2,
            ..Default::default()
          }),
          max: Count {
            value: 3,
            ..Default::default()
          },
          fail: Count {
            value: 4,
            ..Default::default()
          },
        }),
      ),
      (
        "future nope\nfail 0\nmax 1\n",
        Ok(SwapEvents {
          high: None,
          max: Count {
            value: 1,
            ..Default::default()
          },
          fail: Count {
            value: 0,
            ..Default::default()
          },
        }),
      ),
      ("", Err("missing field \"max\"")),
      ("max 1\n", Err("missing field \"fail\"")),
      ("max nope\nfail 0", Err("invalid field \"max\"")),
      ("max 1\nfail -1", Err("invalid field \"fail\"")),
      ("high nope\nmax 1\nfail 0", Err("invalid field \"high\"")),
      ("max 1 extra\nfail 0", Err("excess field")),
    ];

    for (input, expected) in cases {
      match expected {
        | Ok(expected) => {
          assert_eq!(assert_ok!(input.parse::<SwapEvents>()), expected)
        },
        | Err(message) => assert!(
          assert_err!(input.parse::<SwapEvents>())
            .to_string()
            .contains(message),
          "input: {input:?}"
        ),
      }
    }
  }
}
