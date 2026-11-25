//! File capabilities and snapshot types shared by Linux cgroup hierarchies.

pub use cpuset::{CpuList, MemoryNodeList};
pub use device::Device;
pub use hugetlb::PageSize;
pub use interface::{CgroupFile, ReadableFile, WritableFile};
pub use task::TaskId;

pub mod misc;
pub mod rdma;

pub(in crate::cgroup) mod cpuset;
mod device;
mod hugetlb;
mod task;

#[cfg(target_os = "linux")]
pub(in crate::cgroup) mod directory;
#[cfg(target_os = "linux")]
pub(in crate::cgroup) mod discovery;
#[cfg(target_os = "linux")]
mod file;
pub(crate) mod interface;

/// Type parameter identifying cgroup v1 file declarations.
#[derive(Debug)]
#[non_exhaustive]
pub struct V1;

/// Type parameter identifying cgroup v2 file declarations.
#[derive(Debug)]
#[non_exhaustive]
pub struct V2;
