//! Read-only cgroup v2 interfaces. Parsers work on any OS; handles require Linux.
//!
//! ```no_run
//! # #[cfg(target_os = "linux")]
//! use sakai_core::Cgroup;
//! # #[cfg(target_os = "linux")]
//! # fn example() -> Result<(), sakai_core::Error> {
//! let cgroup = Cgroup::from_current_process()?;
//! let usage = cgroup.cpu().stat()?.time().usage();
//! let quota = cgroup.cpu().max()?;
//! # Ok(())
//! # }
//! ```
pub use cgroup::v2;
pub use error::Error;
pub use limit::MaxOr;
pub use pressure::{Pressure, PressureLine};
pub use unit::{Count, EventRate, NonZeroTime, Ratio, Time};
#[cfg(target_os = "linux")]
pub use v2::Cgroup;
pub use v2::cpu::{CpuWeight, Nice, Weight};

pub mod cgroup;
pub mod error;
pub mod limit;
mod parse;
pub mod pressure;
pub mod unit;
