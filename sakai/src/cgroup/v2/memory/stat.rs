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

    MemoryStatByteField::require_baseline(contents, &stat.bytes)?;
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

impl MemoryStatByteField {
  pub(super) fn require_baseline<T>(raw: &str, fields: &BTreeMap<Self, T>) -> Result<(), ParseError<ParseValueError>> {
    for field in [Self::Anon, Self::File] {
      if !fields.contains_key(&field) {
        return Err(ParseError::missing(raw, field.into()));
      }
    }
    Ok(())
  }
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
  use assertables::assert_ok;
  use indoc::indoc;
  use uom::si::information::byte;

  use super::{MemoryStatByteField::*, MemoryStatCountField::*, MemoryStatPageField::*, *};
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

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
        indoc! {"pgscan_direct 7\nfuture_metric nope\nfile 4096\npgfault 3\nanon 8192\n\
        workingset_refault_file 5\nworkingset_refault_anon 6\nworkingset_activate_anon 7\n\
        workingset_activate_file 8\nworkingset_restore_anon 9\nworkingset_restore_file 10\n\
        kernel 1024\nzswap_incomp 4096\npswpin 2\nthp_fault_alloc 1\n"},
        MemoryStat::expected(
          [(Anon, 8192), (File, 4096), (Kernel, 1024), (ZswapIncomp, 4096)],
          [(PgscanDirect, 7), (Pswpin, 2)],
          [
            (WorkingsetRefaultFile, 5),
            (WorkingsetRefaultAnon, 6),
            (WorkingsetActivateAnon, 7),
            (WorkingsetActivateFile, 8),
            (WorkingsetRestoreAnon, 9),
            (WorkingsetRestoreFile, 10),
            (Pgfault, 3),
            (ThpFaultAlloc, 1),
          ],
        ),
      ),
      ("anon 0\nfile 1\n", MemoryStat::expected([(Anon, 0), (File, 1)], [], [])),
      ("anon 1\nfile 2\nanon 3\nfuture 4\nfuture 5\n", MemoryStat::expected([(Anon, 3), (File, 2)], [], [])),
    ] {
      assert_eq!(assert_ok!(input.parse::<MemoryStat>()), expected);
    }
  }

  #[test]
  fn rejects_invalid_stat_forms() {
    Cases::<MemoryStat>::check(
      [
        ("", Missing("anon")),
        ("anon 1", Missing("file")),
        ("anon nope\nfile 1", Invalid("anon", "nope")),
        ("anon -1\nfile 1", Invalid("anon", "-1")),
        ("anon 1\nfile 1\npgscan 18446744073709551616", Invalid("pgscan", "18446744073709551616")),
        ("anon 1\nfile 1\nextra", Missing("value")),
        ("anon 1 2\nfile 1", Excess),
      ]
      .map(|(input, failure)| (input, Err(failure))),
    );
  }
}
