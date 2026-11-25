//! Miscellaneous resource snapshots shared by both hierarchy versions.

use std::collections::BTreeMap;

use crate::{
  Count, MaxOr,
  parse::{ParseCount, cgroup_value},
};

/// Machine-wide available miscellaneous resource capacities.
///
/// Values count resource units keyed by kernel resource names. This stored
/// snapshot can change on the next read. Empty maps
/// are valid; hardware-dependent and future resource names are preserved.
/// See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MiscCapacity {
  /// Reported resource names and their counts of resource units.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  pub resources: BTreeMap<String, Count>,
}

cgroup_value! { map MiscCapacity, resources: ParseCount, "value" }

/// Current hierarchical miscellaneous resource usage.
///
/// Values count resource units keyed by kernel resource names. This stored
/// snapshot can change on the next read. Empty maps
/// are valid; hardware-dependent and future resource names are preserved.
/// Since Linux 6.5, this file also exists at the root; earlier kernels omit it. See
/// [Linux 6.5 source](https://github.com/torvalds/linux/blob/v6.5/kernel/cgroup/misc.c).
/// See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MiscCurrent {
  /// Reported resource names and their counts of resource units.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  pub resources: BTreeMap<String, Count>,
}

cgroup_value! { map MiscCurrent, resources: ParseCount, "value" }

/// Configured hierarchical miscellaneous resource limits.
///
/// Values count resource units keyed by kernel resource names. This stored
/// snapshot can change on the next read. Empty maps
/// are valid; hardware-dependent and future resource names are preserved.
/// `max` means unlimited here; ancestor limits and machine capacity still apply.
/// Zero is valid, and a limit may exceed the separately read machine capacity.
/// See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MiscMax {
  /// Maximum counts per resource.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  pub resources: BTreeMap<String, MaxOr<Count>>,
}

cgroup_value! { map MiscMax, resources: ParseCount, "value" }
pub(in crate::cgroup) fn encode_limit(
  (resource, value): (&str, MaxOr<Count>),
) -> Result<String, crate::error::EncodeError> {
  super::interface::encode_keyed::<ParseCount>(resource, "resource", &value, "limit")
}
