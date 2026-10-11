//! Typed snapshots and read-only handles for the unified cgroup v2 hierarchy.
//!
//! Snapshots implement string parsing on any OS. Handles and borrowed
//! controller readers are available on Linux. See the
//! [Linux cgroup v2 guide](https://docs.kernel.org/admin-guide/cgroup-v2.html).
//!
//! # Interface-read errors
//!
//! Linux interface readers report the following errors:
//!
//! - [`crate::Error::FileMissing`] if the interface is absent, including controller
//!   availability, kernel configuration or version, and hierarchy-root exemptions.
//! - [`crate::Error::NotSupported`] if the kernel reports an unsupported operation.
//! - [`crate::Error::Parse`] if the interface contents are invalid.
//! - [`crate::Error::Operation`] for other operating-system failures, retaining the
//!   operation, diagnostic path, and source error.
//!
//! Reader-specific availability and root exemptions are documented on each interface.

#[cfg(target_os = "linux")]
pub use handle::Cgroup;
#[cfg(target_os = "linux")]
pub use path::{CgroupPath, CgroupPathError};

pub mod core;
pub mod cpu;
pub mod cpuset;
pub mod io;
pub mod memory;
#[cfg(target_os = "linux")]
pub mod notification;
pub mod pids;

#[cfg(target_os = "linux")]
mod file;
#[cfg(target_os = "linux")]
mod handle;
#[cfg(target_os = "linux")]
mod path;
