//! Typed interfaces for legacy Linux cgroup hierarchies.
//!
//! Select a controller or named hierarchy when opening a process's cgroup.
//! Controllers on the same hierarchy share a directory. A hybrid system may
//! also expose a separate handle from [crate::v2] for the same process.
//! Snapshots can be parsed on any OS. File operations require Linux.
//! See the [Linux cgroup v1 guide](https://docs.kernel.org/admin-guide/cgroup-v1/index.html).
#[cfg(target_os = "linux")]
pub use handle::Cgroup;

pub use super::{CgroupFile, ReadableFile, WritableFile, common::V1 as Version};

#[cfg(target_os = "linux")]
mod handle;

pub mod blkio;
pub mod core;
pub mod cpu;
pub mod cpuacct;
pub mod cpuset;
pub mod devices;
pub mod freezer;
pub mod hugetlb;
pub mod memory;
pub mod misc;
pub mod net_cls;
pub mod net_prio;
pub mod pids;
pub mod rdma;

mod codec;
