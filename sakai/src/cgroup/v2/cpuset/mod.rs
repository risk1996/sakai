//! CPU and NUMA memory-node placement snapshots.
//!
//! Configured lists express requests for tasks in this cgroup; effective lists
//! describe the online resources granted by its parent. These are identifiers,
//! not counts or quantities. Reads never change placement or migrate memory.
//! See [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).

pub use list::{CpuList, MemoryNodeList};
pub use partition::CpusetPartition;

use super::interface::cgroup_file;

mod list;
mod partition;

cgroup_file!(alias
  /// Online CPUs granted to tasks in this cgroup, including at the root.
  /// The grant changes with ancestor configuration and hotplug.
  /// An empty list means no CPUs are granted.
  CpusEffective, "cpuset.cpus.effective", Read, CpuList
);

cgroup_file!(alias
  /// CPUs available for exclusive partition allocation on non-root cgroups.
  /// This grant can differ from the request and from [`CpusEffective`].
  /// Since Linux 6.7. See the [Linux 6.7 source](https://github.com/torvalds/linux/blob/v6.7/kernel/cgroup/cpuset.c).
  CpusExclusiveEffective, "cpuset.cpus.exclusive.effective", Read, CpuList
);

cgroup_file!(alias
  /// CPUs belonging to isolated partitions across the hierarchy, exposed at the root.
  /// Since Linux 6.8. See the [Linux 6.8 source](https://github.com/torvalds/linux/blob/v6.8/kernel/cgroup/cpuset.c).
  CpusIsolated, "cpuset.cpus.isolated", Read, CpuList
);

cgroup_file!(alias
  /// Online memory nodes granted to tasks in this cgroup, including at the root.
  /// The grant changes with ancestor configuration and hotplug.
  /// An empty list means no nodes are granted.
  MemsEffective, "cpuset.mems.effective", Read, MemoryNodeList
);

cgroup_file!(alias
  /// CPUs requested for exclusive partition allocation on non-root cgroups.
  /// An empty request falls back to `cpuset.cpus` when forming a partition.
  /// Since Linux 6.7. See the [Linux 6.7 source](https://github.com/torvalds/linux/blob/v6.7/kernel/cgroup/cpuset.c).
  CpusExclusive, "cpuset.cpus.exclusive", ReadWrite, CpuList
);
cgroup_file!(write CpusExclusive, 'a => &'a CpuList, |value: &CpuList| placement_command(value.encode_cgroup()));

fn placement_command(command: Result<String, crate::error::EncodeError>) -> Result<String, crate::error::EncodeError> {
  let mut command = command?;
  if command.is_empty() {
    command.push('\n');
  }
  Ok(command)
}
