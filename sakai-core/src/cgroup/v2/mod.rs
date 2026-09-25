pub mod core;
pub mod cpu;

#[cfg(target_os = "linux")]
mod handle;
#[cfg(target_os = "linux")]
pub use handle::{Cgroup, OpenCgroup};

#[cfg(target_os = "linux")]
mod path;

#[cfg(target_os = "linux")]
pub use path::{CgroupPath, CgroupPathError};

#[cfg(target_os = "linux")]
mod io;
