use std::{collections::BTreeMap, str::FromStr};

use crate::{
  error::{ParseError, ParseValueError},
  parse::{ParseCgroup, ParseCount, Parser},
  unit::{Bytes, Count, Pages},
};

/// A point-in-time breakdown from `memory.stat` for this cgroup and descendants.
///
/// Memory amounts are bytes, page quantities are numbers of pages (not bytes),
/// and event counts are dimensionless. Kernel versions and configurations can
/// omit individual fields; absence differs from a reported zero. Unknown keys
/// are ignored because their units are not defined by this type.
/// See [Linux `memory.stat` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryStat {
  bytes: BTreeMap<MemoryStatByteField, Bytes>,
  pages: BTreeMap<MemoryStatPageField, Pages>,
  counts: BTreeMap<MemoryStatCountField, Count>,
}

impl MemoryStat {
  /// The cgroup v2 `memory.stat` interface filename.
  /// See [Linux `memory.stat` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  pub const FILE_NAME: &'static str = "memory.stat";

  /// Returns memory amounts measured in bytes.
  /// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  #[must_use]
  pub const fn bytes(&self) -> &BTreeMap<MemoryStatByteField, Bytes> { &self.bytes }

  /// Returns quantities measured in pages; page size is not assumed.
  /// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  #[must_use]
  pub const fn pages(&self) -> &BTreeMap<MemoryStatPageField, Pages> { &self.pages }

  /// Returns event and object counts.
  /// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  #[must_use]
  pub const fn counts(&self) -> &BTreeMap<MemoryStatCountField, Count> { &self.counts }

  fn insert(&mut self, raw: &str, key: &str, value: &str) -> Result<(), ParseError<ParseValueError>> {
    if let Ok(field) = MemoryStatByteField::from_str(key) {
      self.bytes.insert(field, Bytes::parse_field(raw, field.into(), value)?);
    } else if let Ok(field) = MemoryStatPageField::from_str(key) {
      self.pages.insert(field, Pages::parse_field(raw, field.into(), value)?);
    } else if let Ok(field) = MemoryStatCountField::from_str(key) {
      self.counts.insert(field, <Count as ParseCgroup<ParseCount>>::parse_field(raw, field.into(), value)?);
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
/// See [Linux `memory.stat` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryStatByteField {
  /// Anonymous memory, such as heaps and stacks, in bytes (`anon`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Anon,
  /// Cached filesystem data, including tmpfs and shared memory, in bytes (`file`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  File,
  /// Total charged kernel memory in bytes (`kernel`); includes kernel stacks, page tables, and slab.
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.18; see the [upstream Linux 5.18 source](https://github.com/torvalds/linux/blob/v5.18/mm/memcontrol.c).
  Kernel,
  /// Memory used by kernel stacks in bytes (`kernel_stack`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  KernelStack,
  /// Memory used by page tables in bytes (`pagetables`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Pagetables,
  /// Secondary page tables, such as KVM or IOMMU tables, in bytes (`sec_pagetables`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.1; see the [upstream Linux 6.1 source](https://github.com/torvalds/linux/blob/v6.1/mm/memcontrol.c).
  SecPagetables,
  /// Memory used by per-CPU kernel allocations in bytes (`percpu`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Percpu,
  /// Memory used by network transmission buffers in bytes (`sock`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Sock,
  /// Memory used by vmalloc kernel allocations in bytes (`vmalloc`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.17; see the [upstream Linux 5.17 source](https://github.com/torvalds/linux/blob/v5.17/mm/memcontrol.c).
  Vmalloc,
  /// Swap-backed cached data, including tmpfs and shared memory, in bytes (`shmem`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Shmem,
  /// Memory occupied by the zswap compression backend in bytes (`zswap`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/mm/memcontrol.c).
  Zswap,
  /// Uncompressed, incompressible memory stored in zswap in bytes (`zswap_incomp`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 7.1; see the [upstream Linux 7.1 source](https://github.com/torvalds/linux/blob/v7.1/mm/memcontrol.c).
  ZswapIncomp,
  /// Application memory stored in zswap, measured before compression, in bytes (`zswapped`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/mm/memcontrol.c).
  Zswapped,
  /// Cached filesystem data mapped into processes in bytes (`file_mapped`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  FileMapped,
  /// Cached filesystem data awaiting writeback in bytes (`file_dirty`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  FileDirty,
  /// Cached filesystem data currently being written back in bytes (`file_writeback`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  FileWriteback,
  /// Memory occupied by the swap cache in bytes (`swapcached`); charged to both memory and swap usage.
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Swapcached,
  /// Anonymous transparent hugepage memory in bytes (`anon_thp`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  AnonThp,
  /// Filesystem cache held in transparent hugepages in bytes (`file_thp`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  FileThp,
  /// Shared-memory transparent hugepage memory in bytes (`shmem_thp`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ShmemThp,
  /// Anonymous and swap-backed memory on the inactive LRU list in bytes (`inactive_anon`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  InactiveAnon,
  /// Anonymous and swap-backed memory on the active LRU list in bytes (`active_anon`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ActiveAnon,
  /// Filesystem cache on the inactive LRU list in bytes (`inactive_file`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  InactiveFile,
  /// Filesystem cache on the active LRU list in bytes (`active_file`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ActiveFile,
  /// Memory that reclaim cannot evict, such as locked memory, in bytes (`unevictable`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Unevictable,
  /// Kernel slab allocations that reclaim can free in bytes (`slab_reclaimable`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  SlabReclaimable,
  /// Kernel slab allocations that reclaim cannot free in bytes (`slab_unreclaimable`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  SlabUnreclaimable,
  /// Total kernel slab memory in bytes (`slab`); includes both reclaimable and unreclaimable slab.
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Slab,
  /// HugeTLB memory charged to this memory controller in bytes (`hugetlb`); requires the `memory_hugetlb_accounting` mount option.
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.13; see the [upstream Linux 6.13 source](https://github.com/torvalds/linux/blob/v6.13/mm/memcontrol.c).
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
/// See [Linux `memory.stat` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryStatPageField {
  /// Pages brought in from swap (`pswpin`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.13; see the [upstream Linux 6.13 source](https://github.com/torvalds/linux/blob/v6.13/mm/memcontrol.c).
  Pswpin,
  /// Pages sent out to swap (`pswpout`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.13; see the [upstream Linux 6.13 source](https://github.com/torvalds/linux/blob/v6.13/mm/memcontrol.c).
  Pswpout,
  /// Pages examined during memory reclaim (`pgscan`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Pgscan,
  /// Pages successfully reclaimed (`pgsteal`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Pgsteal,
  /// Pages examined during reclaim by the background reclaim daemon (`pgscan_kswapd`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.0; see the [upstream Linux 6.0 source](https://github.com/torvalds/linux/blob/v6.0/mm/memcontrol.c).
  PgscanKswapd,
  /// Pages examined during reclaim by direct reclaim in the allocating task (`pgscan_direct`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.0; see the [upstream Linux 6.0 source](https://github.com/torvalds/linux/blob/v6.0/mm/memcontrol.c).
  PgscanDirect,
  /// Pages examined during reclaim by the transparent hugepage collapse daemon (`pgscan_khugepaged`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.2; see the [upstream Linux 6.2 source](https://github.com/torvalds/linux/blob/v6.2/Documentation/admin-guide/cgroup-v2.rst).
  PgscanKhugepaged,
  /// Pages examined during reclaim by proactive reclaim requested through `memory.reclaim` (`pgscan_proactive`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.15; see the [upstream Linux 6.15 source](https://github.com/torvalds/linux/blob/v6.15/Documentation/admin-guide/cgroup-v2.rst).
  PgscanProactive,
  /// Pages successfully reclaimed by the background reclaim daemon (`pgsteal_kswapd`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.0; see the [upstream Linux 6.0 source](https://github.com/torvalds/linux/blob/v6.0/mm/memcontrol.c).
  PgstealKswapd,
  /// Pages successfully reclaimed by direct reclaim in the allocating task (`pgsteal_direct`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.0; see the [upstream Linux 6.0 source](https://github.com/torvalds/linux/blob/v6.0/mm/memcontrol.c).
  PgstealDirect,
  /// Pages successfully reclaimed by the transparent hugepage collapse daemon (`pgsteal_khugepaged`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.2; see the [upstream Linux 6.2 source](https://github.com/torvalds/linux/blob/v6.2/Documentation/admin-guide/cgroup-v2.rst).
  PgstealKhugepaged,
  /// Pages successfully reclaimed by proactive reclaim requested through `memory.reclaim` (`pgsteal_proactive`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.15; see the [upstream Linux 6.15 source](https://github.com/torvalds/linux/blob/v6.15/Documentation/admin-guide/cgroup-v2.rst).
  PgstealProactive,
  /// Pages scanned on an active least-recently-used (LRU) list (`pgrefill`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Pgrefill,
  /// Pages promoted to the active LRU list (`pgactivate`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Pgactivate,
  /// Pages moved to the inactive LRU list (`pgdeactivate`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Pgdeactivate,
  /// Pages marked reclaimable by `MADV_FREE` (`pglazyfree`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Pglazyfree,
  /// Pages reclaimed after being marked by `MADV_FREE` (`pglazyfreed`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Pglazyfreed,
  /// All-zero pages restored from swap without reading a swap device (`swpin_zero`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.12; see the [upstream Linux 6.12 source](https://github.com/torvalds/linux/blob/v6.12/Documentation/admin-guide/cgroup-v2.rst).
  SwpinZero,
  /// All-zero pages swapped out without writing a swap device (`swpout_zero`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.12; see the [upstream Linux 6.12 source](https://github.com/torvalds/linux/blob/v6.12/Documentation/admin-guide/cgroup-v2.rst).
  SwpoutZero,
  /// Pages restored from zswap (`zswpin`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/mm/memcontrol.c).
  Zswpin,
  /// Pages stored in zswap (`zswpout`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/mm/memcontrol.c).
  Zswpout,
  /// Pages written from zswap to a backing swap device (`zswpwb`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.8; see the [upstream Linux 6.8 source](https://github.com/torvalds/linux/blob/v6.8/mm/memcontrol.c).
  Zswpwb,
  /// Pages moved by automatic NUMA balancing (`numa_pages_migrated`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.12; see the [upstream Linux 6.12 source](https://github.com/torvalds/linux/blob/v6.12/Documentation/admin-guide/cgroup-v2.rst).
  NumaPagesMigrated,
  /// Pages whose page-table entries were updated for NUMA hinting (`numa_pte_updates`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.12; see the [upstream Linux 6.12 source](https://github.com/torvalds/linux/blob/v6.12/Documentation/admin-guide/cgroup-v2.rst).
  NumaPteUpdates,
  /// Pages demoted to a slower memory tier during reclaim by the background reclaim daemon (`pgdemote_kswapd`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.12; see the [upstream Linux 6.12 source](https://github.com/torvalds/linux/blob/v6.12/Documentation/admin-guide/cgroup-v2.rst).
  PgdemoteKswapd,
  /// Pages demoted to a slower memory tier during reclaim by direct reclaim in the allocating task (`pgdemote_direct`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.12; see the [upstream Linux 6.12 source](https://github.com/torvalds/linux/blob/v6.12/Documentation/admin-guide/cgroup-v2.rst).
  PgdemoteDirect,
  /// Pages demoted to a slower memory tier during reclaim by the transparent hugepage collapse daemon (`pgdemote_khugepaged`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.12; see the [upstream Linux 6.12 source](https://github.com/torvalds/linux/blob/v6.12/Documentation/admin-guide/cgroup-v2.rst).
  PgdemoteKhugepaged,
  /// Pages demoted to a slower memory tier during reclaim by proactive reclaim requested through `memory.reclaim` (`pgdemote_proactive`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.15; see the [upstream Linux 6.15 source](https://github.com/torvalds/linux/blob/v6.15/Documentation/admin-guide/cgroup-v2.rst).
  PgdemoteProactive,
  /// Pages promoted to a faster memory tier by automatic NUMA balancing (`pgpromote_success`).
  ///
  /// Requires `CONFIG_NUMA_BALANCING`. See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.12; see the [upstream Linux 6.12 source](https://github.com/torvalds/linux/blob/v6.12/mm/memcontrol.c).
  PgpromoteSuccess,
}

/// Keys in `memory.stat` whose values count events or hugepage objects.
/// See [Linux `memory.stat` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryStatCountField {
  /// Refaults of previously evicted anonymous pages (`workingset_refault_anon`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  WorkingsetRefaultAnon,
  /// Refaults of previously evicted filesystem-backed pages (`workingset_refault_file`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  WorkingsetRefaultFile,
  /// Refaulted anonymous pages immediately activated because they were recently used (`workingset_activate_anon`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  WorkingsetActivateAnon,
  /// Refaulted filesystem-backed pages immediately activated because they were recently used (`workingset_activate_file`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  WorkingsetActivateFile,
  /// Refaulted anonymous pages restored to an active working set (`workingset_restore_anon`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  WorkingsetRestoreAnon,
  /// Refaulted filesystem-backed pages restored to an active working set (`workingset_restore_file`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  WorkingsetRestoreFile,
  /// Times a shadow-entry tracking node was reclaimed (`workingset_nodereclaim`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  WorkingsetNodereclaim,
  /// Page faults handled, including minor and major faults (`pgfault`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Pgfault,
  /// Major page faults requiring data to be fetched from storage (`pgmajfault`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  Pgmajfault,
  /// Transparent hugepages allocated to satisfy page faults (`thp_fault_alloc`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ThpFaultAlloc,
  /// Transparent hugepages allocated by collapsing existing pages (`thp_collapse_alloc`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ThpCollapseAlloc,
  /// Transparent hugepages swapped out without splitting (`thp_swpout`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.7; see the [upstream Linux 6.7 source](https://github.com/torvalds/linux/blob/v6.7/mm/memcontrol.c).
  ThpSwpout,
  /// Transparent hugepages split into base pages before swapping out (`thp_swpout_fallback`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.7; see the [upstream Linux 6.7 source](https://github.com/torvalds/linux/blob/v6.7/mm/memcontrol.c).
  ThpSwpoutFallback,
  /// NUMA hinting faults used to decide page placement (`numa_hint_faults`).
  /// See [Linux memory.stat](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.12; see the [upstream Linux 6.12 source](https://github.com/torvalds/linux/blob/v6.12/Documentation/admin-guide/cgroup-v2.rst).
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
          pgpromote_success 11
        "},
        MemoryStat::expected(
          [(Anon, 8192), (File, 4096), (Kernel, 1024), (ZswapIncomp, 4096)],
          [(PgscanDirect, 7), (Pswpin, 2), (PgpromoteSuccess, 11)],
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
