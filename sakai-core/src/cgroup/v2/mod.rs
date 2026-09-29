#[cfg(target_os = "linux")]
pub use handle::Cgroup;
#[cfg(target_os = "linux")]
pub use path::{CgroupPath, CgroupPathError};

pub mod core;
pub mod cpu;

#[cfg(target_os = "linux")]
mod handle;
#[cfg(target_os = "linux")]
mod io;
#[cfg(target_os = "linux")]
mod path;
