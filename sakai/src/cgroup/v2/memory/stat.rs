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
/// omit individual fields. Unknown keys are ignored because their units are
/// not defined by this type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryStat {
  bytes: BTreeMap<MemoryStatByteField, Bytes>,
  pages: BTreeMap<MemoryStatPageField, Pages>,
  counts: BTreeMap<MemoryStatCountField, Count>,
}

impl MemoryStat {
  /// The cgroup v2 `memory.stat` interface filename.
  pub const FILE_NAME: &'static str = "memory.stat";

  /// Returns memory amounts measured in bytes.
  #[must_use]
  pub const fn bytes(&self) -> &BTreeMap<MemoryStatByteField, Bytes> { &self.bytes }

  /// Returns quantities measured in pages; page size is not assumed.
  #[must_use]
  pub const fn pages(&self) -> &BTreeMap<MemoryStatPageField, Pages> { &self.pages }

  /// Returns event and object counts.
  #[must_use]
  pub const fn counts(&self) -> &BTreeMap<MemoryStatCountField, Count> { &self.counts }

  fn insert(&mut self, raw: &str, key: &str, value: &str) -> Result<(), ParseError<ParseValueError>> {
    if let Ok(field) = MemoryStatByteField::from_str(key) {
      self.bytes.insert(field, Bytes::parse_field(raw, field.into(), value)?);
    } else if let Ok(field) = MemoryStatPageField::from_str(key) {
      self.pages.insert(field, Pages::parse_field(raw, field.into(), value)?);
    } else if let Ok(field) = MemoryStatCountField::from_str(key) {
      self.counts.insert(field, Count::parse_field(raw, field.into(), value)?);
    }
    Ok(())
  }
}

impl FromStr for MemoryStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let stat =
      contents.lines().filter(|line| !line.trim().is_empty()).try_fold(Self::default(), |mut stat, line| {
        let (key, value) = Parser::parse_line(contents, line, |parser| {
          Ok((parser.next_raw_field("key")?, parser.next_raw_field("value")?))
        })?;
        stat.insert(contents, key, value)?;
        Ok::<_, Self::Err>(stat)
      })?;

    for (field, name) in [(MemoryStatByteField::Anon, "anon"), (MemoryStatByteField::File, "file")] {
      if !stat.bytes.contains_key(&field) {
        return Err(ParseError::missing(contents, name));
      }
    }
    Ok(stat)
  }
}

/// Keys in `memory.stat` whose values measure bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::IntoStaticStr)]
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
  /// Bytes occupied by incompressible pages held in zswap.
  ZswapIncomp,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum MemoryStatPageField {
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
  NumaPagesMigrated,
  NumaPteUpdates,
  PgdemoteKswapd,
  PgdemoteDirect,
  PgdemoteKhugepaged,
  PgdemoteProactive,
}

/// Keys in `memory.stat` whose values count events or hugepage objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum MemoryStatCountField {
  WorkingsetRefaultAnon,
  WorkingsetRefaultFile,
  WorkingsetActivateAnon,
  WorkingsetActivateFile,
  WorkingsetRestoreAnon,
  WorkingsetRestoreFile,
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

  impl MemoryStat {
    fn expected<const B: usize, const P: usize, const C: usize>(
      bytes: [(MemoryStatByteField, u64); B],
      pages: [(MemoryStatPageField, u64); P],
      counts: [(MemoryStatCountField, u64); C],
    ) -> Self {
      Self {
        bytes: BTreeMap::from(bytes.map(|(field, value)| (field, Bytes::new::<byte>(value)))),
        pages: BTreeMap::from(pages.map(|(field, value)| (field, Pages { value, ..Default::default() }))),
        counts: BTreeMap::from(counts.map(|(field, value)| (field, Count { value, ..Default::default() }))),
      }
    }
  }

  #[test]
  fn parses_full_and_older_stat_forms() {
    for (input, expected) in [
      (
        indoc! {"
          pgscan_direct 7
          future_metric nope
          file 4096
          pgfault 3
          anon 8192
          workingset_refault_file 5
          workingset_refault_anon 6
          workingset_activate_anon 7
          workingset_activate_file 8
          workingset_restore_anon 9
          workingset_restore_file 10
          kernel 1024
          zswap_incomp 4096
          pswpin 2
          thp_fault_alloc 1
        "},
        MemoryStat::expected(
          [
            (MemoryStatByteField::Anon, 8192),
            (MemoryStatByteField::File, 4096),
            (MemoryStatByteField::Kernel, 1024),
            (MemoryStatByteField::ZswapIncomp, 4096),
          ],
          [(MemoryStatPageField::PgscanDirect, 7), (MemoryStatPageField::Pswpin, 2)],
          [
            (MemoryStatCountField::WorkingsetRefaultFile, 5),
            (MemoryStatCountField::WorkingsetRefaultAnon, 6),
            (MemoryStatCountField::WorkingsetActivateAnon, 7),
            (MemoryStatCountField::WorkingsetActivateFile, 8),
            (MemoryStatCountField::WorkingsetRestoreAnon, 9),
            (MemoryStatCountField::WorkingsetRestoreFile, 10),
            (MemoryStatCountField::Pgfault, 3),
            (MemoryStatCountField::ThpFaultAlloc, 1),
          ],
        ),
      ),
      (
        "anon 0\nfile 1\n",
        MemoryStat::expected([(MemoryStatByteField::Anon, 0), (MemoryStatByteField::File, 1)], [], []),
      ),
      (
        "anon 1\nfile 2\nanon 3\nfuture 4\nfuture 5\n",
        MemoryStat::expected([(MemoryStatByteField::Anon, 3), (MemoryStatByteField::File, 2)], [], []),
      ),
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
      ("anon 1\nfile 1\npgscan 18446744073709551616", "invalid field \"pgscan\""),
      ("anon 1\nfile 1\nextra", "missing field \"value\""),
      ("anon 1 2\nfile 1", "excess field \"additional\""),
    ] {
      assert!(assert_err!(input.parse::<MemoryStat>()).to_string().contains(expected), "input: {input:?}");
    }
  }
}
