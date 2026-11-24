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

crate::v2::interface::cgroup_file!(alias
  /// Online CPUs granted to tasks in this cgroup, including at the root.
  /// The grant changes with ancestor configuration and hotplug.
  /// An empty list means no CPUs are granted.
  CpusEffective, CpuList::EFFECTIVE_FILE_NAME, Read, CpuList
);

crate::v2::interface::cgroup_file!(alias
  /// CPUs available for exclusive partition allocation on non-root cgroups.
  /// This grant can differ from the request and from [`CpusEffective`].
  /// Since Linux 6.7. See the [Linux 6.7 source](https://github.com/torvalds/linux/blob/v6.7/kernel/cgroup/cpuset.c).
  CpusExclusiveEffective, CpuList::EXCLUSIVE_EFFECTIVE_FILE_NAME, Read, CpuList
);

crate::v2::interface::cgroup_file!(alias
  /// CPUs belonging to isolated partitions across the hierarchy, exposed at the root.
  /// Since Linux 6.8. See the [Linux 6.8 source](https://github.com/torvalds/linux/blob/v6.8/kernel/cgroup/cpuset.c).
  CpusIsolated, CpuList::ISOLATED_FILE_NAME, Read, CpuList
);

crate::v2::interface::cgroup_file!(alias
  /// Online memory nodes granted to tasks in this cgroup, including at the root.
  /// The grant changes with ancestor configuration and hotplug.
  /// An empty list means no nodes are granted.
  MemsEffective, MemoryNodeList::EFFECTIVE_FILE_NAME, Read, MemoryNodeList
);

crate::v2::interface::cgroup_file!(alias
  /// CPUs requested for exclusive partition allocation on non-root cgroups.
  /// An empty request falls back to `cpuset.cpus` when forming a partition.
  /// Since Linux 6.7. See the [Linux 6.7 source](https://github.com/torvalds/linux/blob/v6.7/kernel/cgroup/cpuset.c).
  CpusExclusive, CpuList::EXCLUSIVE_FILE_NAME, ReadWrite, CpuList
);

crate::v2::interface::cgroup_file!(file CpuList, CpuList::CPUS_FILE_NAME, ReadWrite);
crate::v2::interface::cgroup_file!(read CpuList => CpuList);

crate::v2::interface::cgroup_file!(file MemoryNodeList, MemoryNodeList::MEMS_FILE_NAME, ReadWrite);
crate::v2::interface::cgroup_file!(read MemoryNodeList => MemoryNodeList);

cgroup_file!(write CpuList, 'a => &'a CpuList, |value: &CpuList| placement_command(value.encode_cgroup()));
cgroup_file!(write CpusExclusive, 'a => &'a CpuList, |value: &CpuList| placement_command(value.encode_cgroup()));
cgroup_file!(write MemoryNodeList, 'a => &'a MemoryNodeList, |value: &MemoryNodeList| placement_command(value.encode_cgroup()));
cgroup_file!(write CpusetPartition, 'a => &'a CpusetPartition, |value: &CpusetPartition| Ok(value.state().to_owned()));

fn placement_command(command: Result<String, crate::error::EncodeError>) -> Result<String, crate::error::EncodeError> {
  let mut command = command?;
  if command.is_empty() {
    command.push('\n');
  }
  Ok(command)
}
