//! CPU controller interface files.

pub use idle::*;
pub use max::*;
pub use max_burst::*;
pub use stat::*;
pub use uclamp::*;
pub use weight::*;
pub use weight_nice::*;

use crate::{pressure::Pressure, v2::interface::cgroup_file};

mod idle;
mod max;
mod max_burst;
mod stat;
mod uclamp;
mod weight;
mod weight_nice;

cgroup_file!(alias
  /// CPU pressure snapshots from `cpu.pressure`.
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  CpuPressure, "cpu.pressure", Read, Pressure
);
