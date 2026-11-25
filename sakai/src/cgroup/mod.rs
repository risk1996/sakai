//! Linux control-group hierarchies; see [cgroups(7)](https://man7.org/linux/man-pages/man7/cgroups.7.html).

pub mod v1;
pub mod v2;

pub use interface::{CgroupFile, ReadableFile, WritableFile};

#[cfg(target_os = "linux")]
mod directory;
#[cfg(target_os = "linux")]
mod discovery;
#[cfg(target_os = "linux")]
mod file;
pub(crate) mod interface;
