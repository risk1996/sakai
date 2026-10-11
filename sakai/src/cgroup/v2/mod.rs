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
