use std::{
  collections::BTreeMap,
  str::{FromStr, SplitAsciiWhitespace as Tokens},
};

use super::{MemoryStatByteField, MemoryStatCountField, MemoryStatPageField};
use crate::{
  error::{ParseError, ParseValueError},
  parse::{ParseBytes, ParseCgroup, ParseCount, ParsePages},
  unit::{Bytes, Count, Pages},
};

/// A point-in-time, per-node breakdown from `memory.numa_stat`.
///
/// The outer keys identify the same kinds of quantities as `memory.stat`.
/// Inner keys are NUMA node IDs. Kernel versions and configurations may omit
/// fields, and unknown future keys are ignored because their units are not
/// defined by this type. The file can be absent without NUMA support.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryNumaStat {
  bytes: BTreeMap<MemoryStatByteField, BTreeMap<u32, Bytes>>,
  pages: BTreeMap<MemoryStatPageField, BTreeMap<u32, Pages>>,
  counts: BTreeMap<MemoryStatCountField, BTreeMap<u32, Count>>,
}

impl MemoryNumaStat {
  /// The cgroup v2 `memory.numa_stat` interface filename.
  pub const FILE_NAME: &'static str = "memory.numa_stat";

  /// Returns byte amounts by memory field and NUMA node ID.
  #[must_use]
  pub const fn bytes(&self) -> &BTreeMap<MemoryStatByteField, BTreeMap<u32, Bytes>> { &self.bytes }

  /// Returns page quantities by memory field and NUMA node ID.
  #[must_use]
  pub const fn pages(&self) -> &BTreeMap<MemoryStatPageField, BTreeMap<u32, Pages>> { &self.pages }

  /// Returns event counts by memory field and NUMA node ID.
  #[must_use]
  pub const fn counts(&self) -> &BTreeMap<MemoryStatCountField, BTreeMap<u32, Count>> { &self.counts }

  fn parse_nodes<Unit, T>(
    raw: &str,
    field: &'static str,
    tokens: Tokens<'_>,
  ) -> Result<BTreeMap<u32, T>, ParseError<ParseValueError>>
  where
    T: ParseCgroup<Unit, Error = ParseValueError>, {
    if tokens.clone().next().is_none() {
      return Err(ParseError::missing(raw, "node"));
    }

    tokens
      .map(|token| {
        let (node, value) =
          token.split_once('=').ok_or_else(|| ParseError::invalid(raw, "node", token, ParseValueError::OutOfRange))?;
        let id = node
          .strip_prefix('N')
          .ok_or_else(|| ParseError::invalid(raw, "node", token, ParseValueError::OutOfRange))?
          .parse::<u32>()
          .map_err(|error| ParseError::invalid(raw, "node", token, error.into()))?;
        Ok((id, T::parse_field(raw, field, value)?))
      })
      .collect()
  }

  fn insert(&mut self, raw: &str, key: &str, tokens: Tokens<'_>) -> Result<(), ParseError<ParseValueError>> {
    if let Ok(field) = MemoryStatByteField::from_str(key) {
      self.bytes.insert(field, Self::parse_nodes::<ParseBytes, Bytes>(raw, field.into(), tokens)?);
    } else if let Ok(field) = MemoryStatPageField::from_str(key) {
      self.pages.insert(field, Self::parse_nodes::<ParsePages, Pages>(raw, field.into(), tokens)?);
    } else if let Ok(field) = MemoryStatCountField::from_str(key) {
      self.counts.insert(field, Self::parse_nodes::<ParseCount, Count>(raw, field.into(), tokens)?);
    }
    Ok(())
  }
}

impl FromStr for MemoryNumaStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let stat =
      contents.lines().filter(|line| !line.trim().is_empty()).try_fold(Self::default(), |mut stat, line| {
        let mut fields = line.split_ascii_whitespace();
        let key = fields.next().ok_or_else(|| ParseError::missing(contents, "key"))?;
        stat.insert(contents, key, fields)?;
        Ok::<_, Self::Err>(stat)
      })?;

    MemoryStatByteField::require_baseline(contents, &stat.bytes)?;
    Ok(stat)
  }
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;
  use indoc::indoc;
  use uom::si::information::byte;

  use super::{
    MemoryStatByteField::{Anon, File},
    MemoryStatCountField::WorkingsetRefaultFile,
    MemoryStatPageField::PgdemoteDirect,
    *,
  };
  use crate::parse::tests::{
    Cases,
    Failure::{Invalid, Missing},
  };

  #[test]
  fn parses_numa_stat_groups_and_ignores_unknowns() {
    for (input, expected) in [
      (
        indoc! {"
          file N2=2048 N0=1024
          future_metric N0=nope
          workingset_refault_file N2=3 N0=1
          anon N0=4096 N2=8192
          pgdemote_direct N2=5 N0=0
        "},
        MemoryNumaStat {
          bytes: BTreeMap::from([
            (Anon, BTreeMap::from([(0, Bytes::new::<byte>(4096)), (2, Bytes::new::<byte>(8192))])),
            (File, BTreeMap::from([(0, Bytes::new::<byte>(1024)), (2, Bytes::new::<byte>(2048))])),
          ]),
          pages: BTreeMap::from([(
            PgdemoteDirect,
            BTreeMap::from([
              (0, Pages { value: 0, ..Default::default() }),
              (2, Pages { value: 5, ..Default::default() }),
            ]),
          )]),
          counts: BTreeMap::from([(
            WorkingsetRefaultFile,
            BTreeMap::from([
              (0, Count { value: 1, ..Default::default() }),
              (2, Count { value: 3, ..Default::default() }),
            ]),
          )]),
        },
      ),
      ("anon N0=0\nfile N0=18446744073709551615\n", MemoryNumaStat {
        bytes: BTreeMap::from([
          (Anon, BTreeMap::from([(0, Bytes::new::<byte>(0))])),
          (File, BTreeMap::from([(0, Bytes::new::<byte>(u64::MAX))])),
        ]),
        ..Default::default()
      }),
    ] {
      assert_eq!(assert_ok!(input.parse::<MemoryNumaStat>()), expected);
    }
  }

  #[test]
  fn rejects_invalid_numa_stat_fields() {
    Cases::<MemoryNumaStat>::check(
      [
        ("", Missing("anon")),
        ("anon N0=1", Missing("file")),
        ("anon\nfile N0=1", Missing("node")),
        ("anon N0=1\nfile N0=", Invalid("file", "")),
        ("anon N0=-1\nfile N0=1", Invalid("anon", "-1")),
        ("anon N0=1\nfile N0=18446744073709551616", Invalid("file", "18446744073709551616")),
        ("anon 0=1\nfile N0=1", Invalid("node", "0=1")),
        ("anon N-1=1\nfile N0=1", Invalid("node", "N-1=1")),
        ("anon N0\nfile N0=1", Invalid("node", "N0")),
        ("anon N4294967296=1\nfile N0=1", Invalid("node", "N4294967296=1")),
      ]
      .map(|(input, failure)| (input, Err(failure))),
    );
  }
}
