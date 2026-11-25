//! Membership files common to v1 hierarchies.
//!
//! See the [Linux v1 control group guide](https://docs.kernel.org/admin-guide/cgroup-v1/cgroups.html).

pub use crate::v2::core::TaskId;

v1_file!(alias
  /// Thread group IDs listed in this cgroup. Reads retain kernel order and duplicates.
  CgroupProcs, "cgroup.procs" => Vec<TaskId>, TaskId::parse_list
);

v1_file!(alias
  /// Individual thread IDs listed in this cgroup.
  Tasks, "tasks" => Vec<TaskId>, TaskId::parse_list
);

v1_file!(write CgroupProcs, 'a => TaskId, |id: TaskId| id.encode_cgroup());
v1_file!(write Tasks, 'a => TaskId, |id: TaskId| id.encode_cgroup());
