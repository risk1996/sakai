//! Per-device I/O accounting, pressure, and configuration snapshots.
//!
//! Device numbers identify block devices, not filesystem paths. A device can
//! disappear between reads; no cross-file consistency is implied.

pub use cost::{IoCostControl, IoCostModel, IoCostModelName, IoCostQos, IoDeviceCostModel, IoDeviceCostQos};
pub use latency::IoLatency;
pub use max::{IoDeviceMax, IoMax, IoMaxCommand, IoMaxUpdate};
pub use priority::IoPriorityClass;
pub use stat::{DelayUsage, IoDeviceStat, IoStat};
pub use weight::{IoWeight, IoWeightCommand, Weight};

pub use crate::cgroup::common::Device;
use crate::{Pressure, v2::interface::cgroup_file};

mod cost;
mod latency;
mod max;
mod priority;
mod stat;
mod weight;

cgroup_file!(alias
  /// I/O pressure snapshots from `io.pressure`.
  /// See [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).
  IoPressure, "io.pressure" => Pressure
);
