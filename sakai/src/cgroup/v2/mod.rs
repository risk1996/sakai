//! Typed snapshots and read-only handles for the unified cgroup v2 hierarchy.
//!
//! Snapshots implement string parsing on any OS. Handles and borrowed
//! controller readers are available on Linux. See the
//! [Linux cgroup v2 guide](https://docs.kernel.org/admin-guide/cgroup-v2.html).

#[cfg(target_os = "linux")]
pub use handle::Cgroup;
#[cfg(target_os = "linux")]
pub use path::{CgroupPath, CgroupPathError};

pub mod core;
pub mod cpu;
pub mod io;
pub mod memory;
pub mod pids;

#[cfg(target_os = "linux")]
mod file;
#[cfg(target_os = "linux")]
mod handle;
#[cfg(target_os = "linux")]
mod path;
