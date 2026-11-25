//! CPU usage in the v1 CPU accounting hierarchy.
//!
//! See the [Linux CPU accounting controller](https://docs.kernel.org/admin-guide/cgroup-v1/cpuacct.html).

use super::codec::Nanoseconds;
use crate::{
  Count, Time,
  parse::{ParseCount, cgroup_value},
};

v1_scalar!(
  /// CPU time consumed by this group and its descendants, reported in nanoseconds.
  CpuUsage,
  "cpuacct.usage",
  Time,
  Nanoseconds,
  Read
);

/// User and system CPU time of the cgroup and its descendants in USER_HZ ticks.
///
/// Use the source system's clock tick rate when converting a saved snapshot.
/// The kernel can report slightly stale tick counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct CpuAccountingStat {
  /// Ticks spent executing user code.
  pub user: Count,
  /// Ticks spent executing kernel code.
  pub system: Count,
}

cgroup_value! { keyed CpuAccountingStat, |_| "value", {
  user: required ParseCount => "user",
  system: required ParseCount => "system",
} }
v1_file!(CpuAccountingStat, "cpuacct.stat", Read);
