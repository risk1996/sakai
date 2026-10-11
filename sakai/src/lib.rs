//! Read-only cgroup v2 interfaces. Parsers work on any OS; handles require Linux.
//!
//! ```no_run
//! # #[cfg(target_os = "linux")]
//! use sakai::Cgroup;
//! # #[cfg(target_os = "linux")]
//! # fn example() -> Result<(), sakai::Error> {
//! let cgroup = Cgroup::from_current_process()?;
//! let usage = cgroup.cpu().stat()?.time().usage();
//! let quota = cgroup.cpu().max()?;
//! # Ok(())
//! # }
//! ```
//!
//! Snapshot storage and parser field identifiers are implementation details;
//! use the public interface-specific snapshot types instead.
//!
//! ```compile_fail,E0603
//! use sakai::scalar::Scalar;
//! ```
//!
//! ```compile_fail,E0432
//! use sakai::v2::cpu::CpuStatField;
//! ```
pub use cgroup::v2;
pub use error::Error;
pub use limit::MaxOr;
pub use pressure::{Pressure, PressureLine};
pub use unit::{Bytes, BytesPerSecond, Count, EventRate, NonZeroTime, OperationsPerSecond, Pages, Ratio, Time};
#[cfg(target_os = "linux")]
pub use v2::Cgroup;
pub use v2::cpu::{CpuWeight, Nice, Weight};

pub mod cgroup;
pub mod error;
pub mod limit;
mod parse;
pub mod pressure;
mod scalar;
pub mod unit;
