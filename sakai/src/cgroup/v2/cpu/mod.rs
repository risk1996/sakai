//! CPU controller interface files.

pub use idle::*;
pub use max::*;
pub use max_burst::*;
pub use stat::*;
pub use uclamp::*;
pub use weight::*;
pub use weight_nice::*;

use crate::pressure::Pressure;

mod idle;
mod max;
mod max_burst;
mod stat;
mod uclamp;
mod weight;
mod weight_nice;

crate::v2::interface::cgroup_file!(alias
  /// CPU pressure snapshots.
  CpuPressure, Pressure::CPU_FILE_NAME, Read, Pressure
);
