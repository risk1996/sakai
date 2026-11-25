//! Task accounting and limits in the v1 PID controller.
//!
//! See the [Linux v1 PID controller guide](https://docs.kernel.org/admin-guide/cgroup-v1/pids.html).

use crate::{
  Count, MaxOr,
  parse::{ParseCount, cgroup_value},
};

v1_scalar!(
  /// Current number of threads in this cgroup and its descendants.
  /// This file is absent at the hierarchy root.
  PidsCurrent,
  "pids.current",
  Count,
  ParseCount,
  Read
);

v1_scalar!(
  /// Local thread limit. Ancestors can impose a lower limit.
  /// The kernel reports max for an unlimited value. This file is absent at the root.
  PidsMax,
  "pids.max",
  MaxOr<Count>,
  ParseCount,
  ReadWrite
);

v1_scalar!(
  /// Peak hierarchical task count during the life of this cgroup, outside the root.
  /// Since Linux 6.1. See the [Linux 6.1 PID controller implementation](https://github.com/torvalds/linux/blob/v6.1/kernel/cgroup/pids.c).
  PidsPeak,
  "pids.peak",
  Count,
  ParseCount,
  Read
);

/// Limit events reported by the v1 PID controller outside the hierarchy root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct PidsEvents {
  /// Failed forks in this cgroup caused by a limit in this group or an ancestor.
  pub max: Count,
}

cgroup_value! { keyed PidsEvents, |_| "value", {
  max: required ParseCount => "max",
} }
v1_file!(PidsEvents, "pids.events", Read);
