//! CPU and NUMA memory-node placement snapshots.
//!
//! Configured lists express requests for tasks in this cgroup; effective lists
//! describe the online resources granted by its parent. These are identifiers,
//! not counts or quantities. Reads never change placement or migrate memory.
//! An empty configuration inherits from the nearest ancestor with a nonempty\n//! request, or from all available resources. Configured lists are absent at the\n//! hierarchy root. An empty effective list represents an empty grant.\n//! See [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).

pub use list::{CpuList, MemoryNodeList};
pub use partition::CpusetPartition;

use super::interface::cgroup_file;
use crate::cgroup::common::cpuset::placement_command;

mod list;
mod partition;

cgroup_file!(alias
  /// Online CPUs granted to tasks in this cgroup, including at the root.
  /// The grant changes with ancestor configuration and hotplug.
  /// An empty list means no CPUs are granted.
  CpusEffective, "cpuset.cpus.effective" => CpuList
);

cgroup_file!(alias
  /// CPUs available for exclusive partition allocation on non-root cgroups.
  /// This grant can differ from the request and from [`CpusEffective`].
  /// Since Linux 6.7. See the [Linux 6.7 source](https://github.com/torvalds/linux/blob/v6.7/kernel/cgroup/cpuset.c).
  CpusExclusiveEffective, "cpuset.cpus.exclusive.effective" => CpuList
);

cgroup_file!(alias
  /// CPUs belonging to isolated partitions across the hierarchy, exposed at the root.
  /// Since Linux 6.8. See the [Linux 6.8 source](https://github.com/torvalds/linux/blob/v6.8/kernel/cgroup/cpuset.c).
  CpusIsolated, "cpuset.cpus.isolated" => CpuList
);

cgroup_file!(alias
  /// Online memory nodes granted to tasks in this cgroup, including at the root.
  /// The grant changes with ancestor configuration and hotplug.
  /// An empty list means no nodes are granted.
  MemsEffective, "cpuset.mems.effective" => MemoryNodeList
);

cgroup_file!(alias
  /// CPUs requested for exclusive partition allocation on non-root cgroups.
  /// An empty request falls back to `cpuset.cpus` when forming a partition.
  /// Since Linux 6.7. See the [Linux 6.7 source](https://github.com/torvalds/linux/blob/v6.7/kernel/cgroup/cpuset.c).
  CpusExclusive, "cpuset.cpus.exclusive" => CpuList
);
cgroup_file!(write CpusExclusive, 'a => &'a CpuList, |value: &CpuList| placement_command(value.encode_cgroup()));
