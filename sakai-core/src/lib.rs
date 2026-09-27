//! Read-only cgroup v2 interfaces. Parsers work on any OS; handles require Linux.
//!
//! ```no_run
//! # #[cfg(target_os = "linux")]
//! use sakai_core::{Cgroup, v2::cpu::ReadCpu};
//! # #[cfg(target_os = "linux")]
//! # fn example() -> Result<(), sakai_core::Error> {
//! let cgroup = Cgroup::from_current_process()?;
//! let usage = cgroup.stat()?.time().usage();
//! let quota = cgroup.max()?;
//! # Ok(())
//! # }
//! ```
pub use cgroup::{
  common::{
    error::Error,
    pressure::{Pressure, PressureLine},
    unit::{Count, EventRate, MaxOr, NonZeroTime, Ratio, Time},
  },
  v2,
};
pub use v2::cpu::{CpuWeight, Nice, Weight};
#[cfg(target_os = "linux")]
pub use v2::{Cgroup, OpenCgroup};

pub mod cgroup;
