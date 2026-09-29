use std::{collections::BTreeMap, str::FromStr};

use crate::{
  error::{ParseError, ParseValueError},
  parse::{ParseCgroup, Parser},
  unit::{Bytes, Count, Pages},
};

/// A point-in-time breakdown from `memory.stat` for this cgroup and descendants.
///
/// Memory amounts are bytes, page quantities are numbers of pages (not bytes),
/// and event counts are dimensionless. Kernel versions and configurations can
/// omit individual fields. Unknown numeric keys are retained in [`Self::extra`]
/// without assigning them a unit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryStat {
  bytes: BTreeMap<MemoryStatByteField, Bytes>,
  pages: BTreeMap<MemoryStatPageField, Pages>,
  counts: BTreeMap<MemoryStatCountField, Count>,
  extra: BTreeMap<String, u64>,
}

impl MemoryStat {
  /// Returns memory amounts measured in bytes.
  #[must_use]
  pub const fn bytes(&self) -> &BTreeMap<MemoryStatByteField, Bytes> {
    &self.bytes
  }

  /// Returns quantities measured in pages; page size is not assumed.
  #[must_use]
  pub const fn pages(&self) -> &BTreeMap<MemoryStatPageField, Pages> {
    &self.pages
  }

  /// Returns event and object counts.
  #[must_use]
  pub const fn counts(&self) -> &BTreeMap<MemoryStatCountField, Count> {
    &self.counts
  }

  /// Returns kernel-added fields with unknown units.
  #[must_use]
  pub const fn extra(&self) -> &BTreeMap<String, u64> {
    &self.extra
  }

  fn insert(
    &mut self,
    raw: &str,
    key: &str,
    value: &str,
  ) -> Result<(), ParseError<ParseValueError>> {
    if let Ok(field) = MemoryStatByteField::from_str(key) {
      self
        .bytes
        .insert(field, Bytes::parse_field(raw, field.into(), value)?);
    } else if let Ok(field) = MemoryStatPageField::from_str(key) {
      self
        .pages
        .insert(field, Pages::parse_field(raw, field.into(), value)?);
    } else if let Ok(field) = MemoryStatCountField::from_str(key) {
      self
        .counts
        .insert(field, Count::parse_field(raw, field.into(), value)?);
    } else {
      self.extra.insert(
        key.to_owned(),
        value.parse().map_err(|source| {
          ParseError::invalid(
            raw,
            "value",
            value,
            ParseValueError::Integer(source),
          )
        })?,
      );
    }
    Ok(())
  }
}

impl FromStr for MemoryStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let stat = contents
      .lines()
      .filter(|line| !line.trim().is_empty())
      .try_fold(Self::default(), |mut stat, line| {
        let (key, value) = Parser::parse_line(contents, line, |parser| {
          Ok((
            parser.next_raw_field("key")?,
            parser.next_raw_field("value")?,
          ))
        })?;
        stat.insert(contents, key, value)?;
        Ok::<_, Self::Err>(stat)
      })?;

    for (field, name) in [
      (MemoryStatByteField::Anon, "anon"),
      (MemoryStatByteField::File, "file"),
    ] {
      if !stat.bytes.contains_key(&field) {
        return Err(ParseError::missing(contents, name));
      }
    }
    Ok(stat)
  }
}

/// Keys in `memory.stat` whose values measure bytes.
#[derive(
  Debug,
  Clone,
  Copy,
  PartialEq,
  Eq,
  PartialOrd,
  Ord,
  Hash,
  strum::EnumString,
  strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum MemoryStatByteField {
  Anon,
  File,
  Kernel,
  KernelStack,
  Pagetables,
  SecPagetables,
  Percpu,
  Sock,
  Vmalloc,
  Shmem,
  Zswap,
  Zswapped,
  FileMapped,
  FileDirty,
  FileWriteback,
  Swapcached,
  AnonThp,
  FileThp,
  ShmemThp,
  InactiveAnon,
  ActiveAnon,
  InactiveFile,
  ActiveFile,
  Unevictable,
  SlabReclaimable,
  SlabUnreclaimable,
  Slab,
  Hugetlb,
}

/// Keys in `memory.stat` whose values measure numbers of pages.
#[derive(
  Debug,
  Clone,
  Copy,
  PartialEq,
  Eq,
  PartialOrd,
  Ord,
  Hash,
  strum::EnumString,
  strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum MemoryStatPageField {
  WorkingsetRefaultAnon,
  WorkingsetRefaultFile,
  WorkingsetActivateAnon,
  WorkingsetActivateFile,
  WorkingsetRestoreAnon,
  WorkingsetRestoreFile,
  Pswpin,
  Pswpout,
  Pgscan,
  Pgsteal,
  PgscanKswapd,
  PgscanDirect,
  PgscanKhugepaged,
  PgscanProactive,
  PgstealKswapd,
  PgstealDirect,
  PgstealKhugepaged,
  PgstealProactive,
  Pgrefill,
  Pgactivate,
  Pgdeactivate,
  Pglazyfree,
  Pglazyfreed,
  SwpinZero,
  SwpoutZero,
  Zswpin,
  Zswpout,
  Zswpwb,
  ZswapIncomp,
  NumaPagesMigrated,
  NumaPteUpdates,
  PgdemoteKswapd,
  PgdemoteDirect,
  PgdemoteKhugepaged,
  PgdemoteProactive,
}

/// Keys in `memory.stat` whose values count events or hugepage objects.
#[derive(
  Debug,
  Clone,
  Copy,
  PartialEq,
  Eq,
  PartialOrd,
  Ord,
  Hash,
  strum::EnumString,
  strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum MemoryStatCountField {
  WorkingsetNodereclaim,
  Pgfault,
  Pgmajfault,
  ThpFaultAlloc,
  ThpCollapseAlloc,
  ThpSwpout,
  ThpSwpoutFallback,
  NumaHintFaults,
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use indoc::indoc;
  use uom::si::information::byte;

  use super::*;

  #[test]
  fn parses_full_and_older_stat_forms() {
    for (input, expected) in [
      (
        indoc! {"
          pgscan_direct 7
          future_metric 99
          file 4096
          pgfault 3
          anon 8192
          workingset_refault_file 5
          kernel 1024
          zswap_incomp 2
          thp_fault_alloc 1
        "},
        MemoryStat {
          bytes: BTreeMap::from([
            (MemoryStatByteField::Anon, Bytes::new::<byte>(8192)),
            (MemoryStatByteField::File, Bytes::new::<byte>(4096)),
            (MemoryStatByteField::Kernel, Bytes::new::<byte>(1024)),
          ]),
          pages: BTreeMap::from([
            (MemoryStatPageField::PgscanDirect, Pages {
              value: 7,
              ..Default::default()
            }),
            (MemoryStatPageField::WorkingsetRefaultFile, Pages {
              value: 5,
              ..Default::default()
            }),
            (MemoryStatPageField::ZswapIncomp, Pages {
              value: 2,
              ..Default::default()
            }),
          ]),
          counts: BTreeMap::from([
            (MemoryStatCountField::Pgfault, Count {
              value: 3,
              ..Default::default()
            }),
            (MemoryStatCountField::ThpFaultAlloc, Count {
              value: 1,
              ..Default::default()
            }),
          ]),
          extra: BTreeMap::from([("future_metric".to_owned(), 99)]),
        },
      ),
      ("anon 0\nfile 1\n", MemoryStat {
        bytes: BTreeMap::from([
          (MemoryStatByteField::Anon, Bytes::new::<byte>(0)),
          (MemoryStatByteField::File, Bytes::new::<byte>(1)),
        ]),
        ..Default::default()
      }),
      ("anon 1\nfile 2\nanon 3\nfuture 4\nfuture 5\n", MemoryStat {
        bytes: BTreeMap::from([
          (MemoryStatByteField::Anon, Bytes::new::<byte>(3)),
          (MemoryStatByteField::File, Bytes::new::<byte>(2)),
        ]),
        extra: BTreeMap::from([("future".to_owned(), 5)]),
        ..Default::default()
      }),
    ] {
      assert_eq!(assert_ok!(input.parse::<MemoryStat>()), expected);
    }
  }

  #[test]
  fn rejects_invalid_stat_forms() {
    for (input, expected) in [
      ("", "missing field \"anon\""),
      ("anon 1", "missing field \"file\""),
      ("anon nope\nfile 1", "invalid field \"anon\""),
      ("anon -1\nfile 1", "invalid field \"anon\""),
      (
        "anon 1\nfile 1\npgscan 18446744073709551616",
        "invalid field \"pgscan\"",
      ),
      ("anon 1\nfile 1\nfuture nope", "invalid field \"value\""),
      (
        "anon 1\nfile 1\nfuture 18446744073709551616",
        "invalid field \"value\"",
      ),
      ("anon 1\nfile 1\nextra", "missing field \"value\""),
      ("anon 1 2\nfile 1", "excess field \"additional\""),
    ] {
      assert!(
        assert_err!(input.parse::<MemoryStat>())
          .to_string()
          .contains(expected),
        "input: {input:?}"
      );
    }
  }
}
