//! Compressed swap accounting and settings in the memory controller.

use std::str::FromStr;

#[cfg(target_os = "linux")]
use super::super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{ParseBoolean, ParseBytes, Parser},
  unit::Bytes,
};

/// A borrowed view of one cgroup's compressed swap interfaces.
///
/// Each method reads a fresh snapshot. Kernels without zswap can lack these
/// files, which returns [`Error::FileMissing`].
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Zswap<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Zswap<'_> {
  /// Memory consumed by this cgroup's zswap compression backend.
  pub fn current(&self) -> Result<ZswapCurrent, Error> {
    self.cgroup.parse(ZswapCurrent::FILE_NAME)
  }

  /// Hard limit on this cgroup's compressed swap pool.
  pub fn max(&self) -> Result<ZswapMax, Error> {
    self.cgroup.parse(ZswapMax::FILE_NAME)
  }

  /// Whether disk swap writeback is enabled for this cgroup.
  pub fn writeback(&self) -> Result<ZswapWriteback, Error> {
    self.cgroup.parse(ZswapWriteback::FILE_NAME)
  }
}

/// Memory consumed by the zswap compression backend in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ZswapCurrent {
  value: Bytes,
}

impl ZswapCurrent {
  /// The cgroup v2 `memory.zswap.current` interface filename.
  pub const FILE_NAME: &'static str = "memory.zswap.current";

  /// Returns compressed pool usage in bytes.
  #[must_use]
  pub const fn value(self) -> Bytes {
    self.value
  }
}

impl FromStr for ZswapCurrent {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("current")?,
      })
    })
  }
}

/// Hard zswap pool limit reported by `memory.zswap.max`.
///
/// `max` means no limit is configured here; ancestor limits can still apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ZswapMax {
  value: MaxOr<Bytes>,
}

impl ZswapMax {
  /// The cgroup v2 `memory.zswap.max` interface filename.
  pub const FILE_NAME: &'static str = "memory.zswap.max";

  /// Returns the pool limit in bytes, or [`MaxOr::Max`].
  #[must_use]
  pub const fn value(self) -> MaxOr<Bytes> {
    self.value
  }
}

impl FromStr for ZswapMax {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBytes, _>("max")?,
      })
    })
  }
}

/// Disk swap writeback policy reported by `memory.zswap.writeback`.
///
/// Disabling writeback also disables swapping to disk when a zswap store
/// fails. An ancestor that disables writeback also disables it for children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ZswapWriteback {
  value: bool,
}

impl ZswapWriteback {
  /// The cgroup v2 `memory.zswap.writeback` interface filename.
  pub const FILE_NAME: &'static str = "memory.zswap.writeback";

  /// Returns this cgroup's configured disk swap writeback policy.
  #[must_use]
  pub const fn value(self) -> bool {
    self.value
  }
}

impl FromStr for ZswapWriteback {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParseBoolean, _>("writeback")?,
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
  fn parses_zswap_current() {
    for (input, expected) in [
      (
        "0\n",
        Ok(ZswapCurrent {
          value: Bytes::new::<byte>(0),
        }),
      ),
      (
        "1048576",
        Ok(ZswapCurrent {
          value: Bytes::new::<mebibyte>(1),
        }),
      ),
      (
        "18446744073709551615",
        Ok(ZswapCurrent {
          value: Bytes::new::<byte>(u64::MAX),
        }),
      ),
      ("", Err("missing field \"current\"")),
      ("1 2", Err("excess field")),
      ("max", Err("invalid field \"current\"")),
      ("-1", Err("invalid field \"current\"")),
      ("18446744073709551616", Err("invalid field \"current\"")),
    ] {
      match expected {
        | Ok(expected) => {
          assert_eq!(assert_ok!(input.parse::<ZswapCurrent>()), expected)
        },
        | Err(message) => assert!(
          assert_err!(input.parse::<ZswapCurrent>())
            .to_string()
            .contains(message),
          "input: {input:?}"
        ),
      }
    }
  }

  #[test]
  fn parses_zswap_max() {
    for (input, expected) in [
      ("max\n", Ok(ZswapMax { value: MaxOr::Max })),
      (
        "0",
        Ok(ZswapMax {
          value: MaxOr::Value(Bytes::new::<byte>(0)),
        }),
      ),
      (
        "1048576",
        Ok(ZswapMax {
          value: MaxOr::Value(Bytes::new::<mebibyte>(1)),
        }),
      ),
      ("", Err("missing field \"max\"")),
      ("max 1", Err("excess field")),
      ("-1", Err("invalid field \"max\"")),
      ("18446744073709551616", Err("invalid field \"max\"")),
    ] {
      match expected {
        | Ok(expected) => {
          assert_eq!(assert_ok!(input.parse::<ZswapMax>()), expected)
        },
        | Err(message) => assert!(
          assert_err!(input.parse::<ZswapMax>())
            .to_string()
            .contains(message),
          "input: {input:?}"
        ),
      }
    }
  }

  #[test]
  fn parses_zswap_writeback() {
    for (input, expected) in [
      ("0\n", Ok(ZswapWriteback { value: false })),
      ("1", Ok(ZswapWriteback { value: true })),
      ("", Err("missing field \"writeback\"")),
      ("1 0", Err("excess field")),
      ("2", Err("invalid field \"writeback\"")),
      ("-1", Err("invalid field \"writeback\"")),
      ("true", Err("invalid field \"writeback\"")),
    ] {
      match expected {
        | Ok(expected) => {
          assert_eq!(assert_ok!(input.parse::<ZswapWriteback>()), expected)
        },
        | Err(message) => assert!(
          assert_err!(input.parse::<ZswapWriteback>())
            .to_string()
            .contains(message),
          "input: {input:?}"
        ),
      }
    }
  }
}
