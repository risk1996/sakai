pub mod cpu;

#[cfg(target_os = "linux")]
mod path;

#[cfg(target_os = "linux")]
pub use path::{CgroupPath, CgroupPathError};

#[cfg(target_os = "linux")]
#[cfg_attr(
  not(test),
  expect(dead_code, reason = "used by Linux cgroup readers as they are added")
)]
mod io;
