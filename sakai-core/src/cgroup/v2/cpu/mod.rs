//! CPU controller interface files.

mod idle;
mod max;
mod max_burst;
mod stat;
mod uclamp;
mod weight;
mod weight_nice;

pub use idle::*;
pub use max::*;
pub use max_burst::*;
pub use stat::*;
pub use uclamp::*;
pub use weight::*;
pub use weight_nice::*;

pub use crate::cgroup::common::pressure::{Pressure, PressureLine};
use crate::cgroup::common::{
  error::Error,
  unit::{MaxOr, Ratio, Time},
};

/// Reads fresh CPU snapshots. Missing optional interfaces return
/// [`Error::FileMissing`], including on old kernels and controller-less mounts.
/// Separate methods do not form an atomic snapshot.
pub trait ReadCpu {
  /// Live accounting, including descendants; available with the controller off.
  fn stat(&self) -> Result<CpuStat, Error>;
  /// Live local runqueue throttling, including inherited bandwidth limits.
  fn stat_local(&self) -> Result<CpuStatLocal, Error>;
  /// Configuration snapshot: idle scheduling or relative weight.
  fn weight(&self) -> Result<CpuWeight, Error>;
  /// Configuration snapshot: coarse nice representation of weight.
  fn weight_nice(&self) -> Result<Nice, Error>;
  /// Configuration snapshot: this cgroup's quota and period, not ancestor limits.
  fn max(&self) -> Result<CpuMax, Error>;
  /// Configuration snapshot: CPU burst allowance as a duration.
  fn max_burst(&self) -> Result<Time, Error>;
  /// Live PSI averages and totals. Never registers a pressure trigger.
  fn pressure(&self) -> Result<Pressure, Error>;
  /// Configuration snapshot: requested utilization floor, not guaranteed bandwidth.
  fn uclamp_min(&self) -> Result<Ratio, Error>;
  /// Configuration snapshot: requested utilization ceiling.
  fn uclamp_max(&self) -> Result<MaxOr<Ratio>, Error>;
  /// Configuration snapshot: whether the cgroup uses idle scheduling.
  fn idle(&self) -> Result<bool, Error>;
}

#[cfg(target_os = "linux")]
impl ReadCpu for super::Cgroup {
  fn stat(&self) -> Result<CpuStat, Error> {
    self.parse("cpu.stat")
  }

  fn stat_local(&self) -> Result<CpuStatLocal, Error> {
    self.parse("cpu.stat.local")
  }

  fn weight(&self) -> Result<CpuWeight, Error> {
    self.parse("cpu.weight")
  }

  fn weight_nice(&self) -> Result<Nice, Error> {
    self.parse("cpu.weight.nice")
  }

  fn max(&self) -> Result<CpuMax, Error> {
    self.parse("cpu.max")
  }

  fn max_burst(&self) -> Result<Time, Error> {
    self
      .parse::<CpuMaxBurst>("cpu.max.burst")
      .map(CpuMaxBurst::value)
  }

  fn pressure(&self) -> Result<Pressure, Error> {
    self.parse("cpu.pressure")
  }

  fn uclamp_min(&self) -> Result<Ratio, Error> {
    self
      .parse::<CpuUclampMin>("cpu.uclamp.min")
      .map(CpuUclampMin::value)
  }

  fn uclamp_max(&self) -> Result<MaxOr<Ratio>, Error> {
    self
      .parse::<CpuUclampMax>("cpu.uclamp.max")
      .map(CpuUclampMax::value)
  }

  fn idle(&self) -> Result<bool, Error> {
    self.parse::<CpuIdle>("cpu.idle").map(CpuIdle::value)
  }
}
