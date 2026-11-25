//! Typed snapshots and handles for the unified cgroup v2 hierarchy.
//!
//! Snapshots implement string parsing on any OS. Typed file I/O is available on Linux. See the
//! [Linux cgroup v2 guide](https://docs.kernel.org/admin-guide/cgroup-v2.html).
//!
//! Import [`CgroupFile`] to access an interface's `FILE_NAME`.
//! [`ReadableFile`] and [`WritableFile`] describe the operations exposed by Sakai.
//! Pressure snapshots implement [`ReadableFile`]. Installing a PSI trigger
//! would require a separate descriptor-scoped API.
//!
//! On Linux, the typed operations are also available directly on the handle.
//!
//! ```no_run
//! #[cfg(target_os = "linux")]
//! fn restore_limit(
//!   cgroup: &sakai::Cgroup,
//!   limit: &sakai::v2::cpu::CpuMax,
//! ) -> Result<(), sakai::Error> {
//!   cgroup.write_value::<sakai::v2::cpu::CpuMax>(limit)
//! }
//! ```
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
//!
//! # Interface-write errors
//!
//! Setters encode a full command before opening the interface. They report
//! [`crate::Error::Encode`] for values that cannot be expressed as a command,
//! [`crate::Error::FileMissing`] for an absent interface, and
//! [`crate::Error::NotSupported`] when the kernel reports an unsupported
//! operation. Other failures, including kernel rejection, permission errors,
//! a read-only mount, and incomplete writes, return [`crate::Error::Operation`].
//! A successful write means the kernel accepted the command. A separate read
//! can observe kernel normalization or a concurrent change.

#[cfg(target_os = "linux")]
pub use handle::Cgroup;

pub use super::{CgroupFile, ReadableFile, WritableFile};

/// Type parameter selecting cgroup v2 file declarations.
#[derive(Debug)]
#[non_exhaustive]
pub struct Version;
#[cfg(target_os = "linux")]
pub use path::{CgroupPath, CgroupPathError};

pub mod core;
pub mod cpu;
pub mod cpuset;
pub mod dmem;
pub mod hugetlb;
pub mod io;
pub mod memory;
pub mod misc;
#[cfg(target_os = "linux")]
pub mod notification;
pub mod pids;
pub mod rdma;

#[cfg(target_os = "linux")]
mod handle;
pub(crate) use super::interface;
#[cfg(target_os = "linux")]
mod path;
