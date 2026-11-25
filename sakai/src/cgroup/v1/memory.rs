//! Legacy memory and combined memory/swap accounting and limits.
//!
//! See the [Linux v1 memory controller guide](https://docs.kernel.org/admin-guide/cgroup-v1/memory.html).

use std::str::FromStr;

use super::codec::encode_byte_limit;
use crate::{
  Bytes, Count, MaxOr,
  error::{EncodeError, ParseError, ParseValueError},
  parse::{KeyedFields, ParseBytes, ParseCount, cgroup_value, encode_pair},
};

v1_scalars! {
  /// Approximate memory usage in bytes, including accounted descendants.
  MemoryUsage, "memory.usage_in_bytes", Bytes, ParseBytes, Read;
  /// Peak accounted memory usage in bytes.
  MemoryPeak, "memory.max_usage_in_bytes", Bytes, ParseBytes, Read;
  /// Local memory limit in bytes. The snapshot preserves the kernel's numeric
  /// unlimited sentinel. Use MaxOr::Max as a write command to reset the limit.
  /// Writes can be rounded to the system page size and are unavailable at the root.
  MemoryLimit, "memory.limit_in_bytes", Bytes, ParseBytes, Read;
}
v1_file!(write MemoryLimit, 'a => MaxOr<Bytes>, |limit| encode_byte_limit(limit, "memory.limit_in_bytes"));

v1_scalars! {
  /// Number of times memory charges encountered this group's limit.
  MemoryFailures, "memory.failcnt", Count, ParseCount, Read;
  /// Approximate combined memory and swap usage in bytes, including accounted descendants.
  /// Requires kernel support for combined memory and swap accounting.
  MemorySwapUsage, "memory.memsw.usage_in_bytes", Bytes, ParseBytes, Read;
  /// Peak combined memory and swap usage in bytes, when accounted by the kernel.
  MemorySwapPeak, "memory.memsw.max_usage_in_bytes", Bytes, ParseBytes, Read;
  /// Local limit for memory plus swap in bytes, when accounted by the kernel.
  /// The snapshot preserves the kernel's numeric unlimited sentinel. Use
  /// MaxOr::Max as a write command to reset the limit.
  MemorySwapLimit, "memory.memsw.limit_in_bytes", Bytes, ParseBytes, Read;
}
v1_file!(write MemorySwapLimit, 'a => MaxOr<Bytes>, |limit| encode_byte_limit(limit, "memory.memsw.limit_in_bytes"));

v1_scalars! {
  /// Number of times charges encountered this group's combined memory and swap limit.
  MemorySwapFailures, "memory.memsw.failcnt", Count, ParseCount, Read;
}

/// Usage and paging counters reported by the v1 memory controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MemoryCounters {
  /// Page cache in bytes.
  pub cache: Bytes,
  /// Anonymous and swap cache memory in bytes.
  pub rss: Bytes,
  /// Anonymous transparent huge pages in bytes when reported.
  pub rss_huge: Option<Bytes>,
  /// Mapped file cache in bytes, including tmpfs and shared memory.
  pub mapped_file: Bytes,
  /// Swap usage in bytes when reported.
  pub swap: Option<Bytes>,
  /// Bytes in the swap cache when reported.
  pub swap_cached: Option<Bytes>,
  /// Dirty memory in bytes.
  pub dirty: Bytes,
  /// Memory queued for writeback in bytes.
  pub writeback: Bytes,
  /// Inactive anonymous memory in bytes.
  pub inactive_anon: Bytes,
  /// Active anonymous memory in bytes.
  pub active_anon: Bytes,
  /// Inactive file-backed memory and lazy-free anonymous pages in bytes.
  pub inactive_file: Bytes,
  /// Active file-backed memory in bytes.
  pub active_file: Bytes,
  /// Unevictable memory in bytes.
  pub unevictable: Bytes,
  /// Charging events when reported.
  pub page_charges: Option<Count>,
  /// Uncharging events when reported.
  pub page_uncharges: Option<Count>,
  /// Page faults.
  pub page_faults: Count,
  /// Major page faults.
  pub major_page_faults: Count,
}

cgroup_value! { fields MemoryCounters, {
  cache: required ParseBytes => "cache",
  rss: required ParseBytes => "rss",
  rss_huge: optional ParseBytes => "rss_huge",
  mapped_file: required ParseBytes => "mapped_file",
  swap: optional ParseBytes => "swap",
  swap_cached: optional ParseBytes => "swapcached",
  dirty: required ParseBytes => "dirty",
  writeback: required ParseBytes => "writeback",
  inactive_anon: required ParseBytes => "inactive_anon",
  active_anon: required ParseBytes => "active_anon",
  inactive_file: required ParseBytes => "inactive_file",
  active_file: required ParseBytes => "active_file",
  unevictable: required ParseBytes => "unevictable",
  page_charges: optional ParseCount => "pgpgin",
  page_uncharges: optional ParseCount => "pgpgout",
  page_faults: required ParseCount => "pgfault",
  major_page_faults: required ParseCount => "pgmajfault",
} }

/// Local and hierarchical accounting from the volatile v1 memory.stat file.
/// Unknown keys are ignored. Optional counters depend on kernel support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MemoryStat {
  /// Counters for this cgroup alone.
  pub local: MemoryCounters,
  /// Counters for this cgroup and its accounted descendants.
  pub total: MemoryCounters,
  /// Hierarchical memory limit in bytes, including the raw unlimited sentinel.
  pub hierarchical_memory_limit: Bytes,
  /// Hierarchical memory and swap limit in bytes when reported.
  pub hierarchical_memsw_limit: Option<Bytes>,
}

impl FromStr for MemoryStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(contents, |_| "value")?;
    let totals = KeyedFields::new(
      contents,
      fields.iter().filter_map(|(key, value)| key.strip_prefix("total_").map(|key| Ok((key, value)))),
    )?;
    Ok(Self {
      local: MemoryCounters::from_fields(&fields)?,
      total: MemoryCounters::from_fields(&totals)?,
      hierarchical_memory_limit: fields.required::<ParseBytes, _>("hierarchical_memory_limit")?,
      hierarchical_memsw_limit: fields.optional::<ParseBytes, _>("hierarchical_memsw_limit")?,
    })
  }
}

v1_file!(MemoryStat, "memory.stat", Read);

impl MemoryStat {
  /// Encodes the snapshot as v1 memory.stat text. Unrecognized fields are omitted.
  ///
  /// # Errors
  ///
  /// Returns an error if a value cannot be represented in the kernel format.
  pub fn encode_cgroup(&self) -> Result<String, EncodeError> {
    let mut output = String::new();
    self.local.encode_fields(&mut output, "\n", ' ')?;
    encode_pair::<ParseBytes, _>(&mut output, "\n", ' ', "hierarchical_memory_limit", &self.hierarchical_memory_limit)?;
    if let Some(limit) = &self.hierarchical_memsw_limit {
      encode_pair::<ParseBytes, _>(&mut output, "\n", ' ', "hierarchical_memsw_limit", limit)?;
    }
    let mut totals = String::new();
    self.total.encode_fields(&mut totals, "\n", ' ')?;
    for line in totals.lines() {
      output.push_str("\ntotal_");
      output.push_str(line);
    }
    Ok(output)
  }
}
