//! Read-only cgroup v2 interfaces. Parsers work on any OS; handles require Linux.
//!
//! `sakai` reads CPU, memory, task-count, I/O, and core cgroup state. Use
//! `Cgroup` on Linux to discover a cgroup, then select a controller reader.
//! Each read returns an owned snapshot: getters inspect that snapshot without
//! doing more I/O. Reads of different files are not an atomic transaction.
//!
//! # Units and missing values
//!
//! Durations use [`Time`] (stored in nanoseconds), memory amounts use [`Bytes`],
//! and task or event totals use [`Count`]. Kernel percentages become [`Ratio`],
//! where 1.0 means 100%. [`MaxOr::Max`] means no limit at this cgroup; an
//! ancestor can still impose a limit. An optional counter of `None` means it
//! was not reported, rather than a measured zero.
//!
//! The public quantities intentionally expose `uom` interoperability: construct
//! and convert them with `uom`'s unit types. [`Count`] and [`unit::Pages`] use
//! their public `value` field. Map accessors intentionally return borrowed
//! `BTreeMap` values for deterministic iteration and typed lookup; absent keys
//! mean unreported fields, not zero. These representations are public contracts.
//! Use [`NonZeroTime::event_rate`] for fractional monitoring rates.
//!
//! # Linux availability
//!
//! Linux 5.15 is the oldest kernel in the project's test matrix. Interfaces
//! and fields added after 5.15 have a "Since Linux" note with an upstream
//! source link. These versions describe mainline Linux: distributions may
//! backport features, and kernel configuration, disabled controllers, and
//! hierarchy-root exemptions can still make a file unavailable. Missing files
//! return [`Error::FileMissing`]. Parsing failures retain their path and cause.
//!
//! The authoritative references are the
//! [Linux cgroup v2 guide](https://docs.kernel.org/admin-guide/cgroup-v2.html) and
//! [cgroups(7)](https://man7.org/linux/man-pages/man7/cgroups.7.html).
//! Pressure metrics follow [Linux PSI](https://docs.kernel.org/accounting/psi.html).
//!
//! # Read a cgroup
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
//! # Parse saved interface contents
//!
//! Parsing does not open a cgroup or require Linux. The same snapshot types
//! are returned by live controller readers.
//!
//! ```
//! use sakai::{MaxOr, v2::cpu::CpuMax};
//! use uom::si::ratio::ratio;
//!
//! let limit: CpuMax = "25000 100000".parse()?;
//! let MaxOr::Value(cpus) = limit.cpu_count() else {
//!   return Err("expected a finite quota".into());
//! };
//! assert_eq!(cpus.get::<ratio>(), 0.25);
//! # Ok::<(), Box<dyn std::error::Error>>(())
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
#![warn(missing_docs)]

pub use cgroup::v2;
pub use error::Error;
pub use limit::MaxOr;
pub use pressure::{Pressure, PressureLine};
pub use unit::{
  Bytes, BytesPerSecond, Count, EventRate, FractionalEventRate, NonZeroTime, OperationsPerSecond, Pages, Ratio, Time,
};
#[cfg(target_os = "linux")]
pub use v2::Cgroup;
pub use v2::cpu::{CpuWeight, Nice, Weight};

/// Cgroup hierarchy interfaces; currently the read-only v2 hierarchy.
pub mod cgroup;
/// Discovery, operating-system, and typed parsing errors.
pub mod error;
/// Limits that accept the kernel's special `max` token.
pub mod limit;
mod parse;
/// Shared pressure stall information for CPU, memory, and I/O.
pub mod pressure;
mod scalar;
// Keep the packaged consumer examples covered by the workspace doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
mod readme {}
/// Quantities that preserve the units of cgroup values.
pub mod unit;
