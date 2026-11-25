//! CPU and NUMA memory-node placement in v1 cpuset mounts.
//!
//! See the [Linux v1 cpuset guide](https://docs.kernel.org/admin-guide/cgroup-v1/cpusets.html).

pub use crate::cgroup::common::{CpuList, MemoryNodeList};
use crate::{
  cgroup::common::{
    cpuset::placement_command,
    interface::{cgroup_file, cgroup_scalars},
  },
  parse::ParseBoolean,
};

cgroup_file!(alias
  /// Configured CPU IDs. Hotplug may change the list unless cpuset_v2_mode is used.
  Cpus, "cpuset.cpus" => CpuList
; super::Version);
cgroup_file!(write Cpus, 'a => &'a CpuList, |value: &CpuList| placement_command(value.encode_cgroup()));
cgroup_file!(alias
  /// Configured memory node IDs. Hotplug may change the list unless cpuset_v2_mode is used.
  Mems, "cpuset.mems" => MemoryNodeList
; super::Version);
cgroup_file!(write Mems, 'a => &'a MemoryNodeList, |value: &MemoryNodeList| placement_command(value.encode_cgroup()));
cgroup_file!(alias
  /// CPU IDs available to this cgroup. The list follows hotplug with cpuset_v2_mode.
  EffectiveCpus, "cpuset.effective_cpus" => CpuList
; super::Version);
cgroup_file!(alias
  /// Memory node IDs available to this cgroup. The list follows hotplug with cpuset_v2_mode.
  EffectiveMems, "cpuset.effective_mems" => MemoryNodeList
; super::Version);

cgroup_scalars! {
  super::Version;
  /// Whether CPU assignments are exclusive of cgroups outside the ancestor and descendant chains.
  CpuExclusive, "cpuset.cpu_exclusive", bool, ParseBoolean, ReadWrite;
  /// Whether memory-node assignments are exclusive of cgroups outside the ancestor and descendant chains.
  MemExclusive, "cpuset.mem_exclusive", bool, ParseBoolean, ReadWrite;
  /// Whether the kernel migrates pages when memory-node placement changes.
  MemoryMigrate, "cpuset.memory_migrate", bool, ParseBoolean, ReadWrite;
  /// Whether the scheduler load balances the CPUs in this cpuset.
  SchedLoadBalance, "cpuset.sched_load_balance", bool, ParseBoolean, ReadWrite;
}
