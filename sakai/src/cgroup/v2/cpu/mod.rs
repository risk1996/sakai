//! CPU controller interface files.

pub use idle::*;
pub use max::*;
pub use max_burst::*;
pub use stat::*;
pub use uclamp::*;
pub use weight::*;
pub use weight_nice::*;

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::{error::Error, pressure::Pressure};

mod idle;
mod max;
mod max_burst;
mod stat;
mod uclamp;
mod weight;
mod weight_nice;

/// A borrowed view of one open cgroup's CPU interfaces.
///
/// Each method reads a fresh snapshot through the cgroup's directory. Missing
/// optional interfaces return [`Error::FileMissing`]; separate reads are not
/// atomic together.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Cpu<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Cpu<'_> {
  /// Live accounting, including descendants; available with the controller off.
  pub fn stat(&self) -> Result<CpuStat, Error> {
    self.cgroup.parse("cpu.stat")
  }

  /// Live local runqueue throttling, including inherited bandwidth limits.
  pub fn stat_local(&self) -> Result<CpuStatLocal, Error> {
    self.cgroup.parse("cpu.stat.local")
  }

  /// Configuration snapshot: idle scheduling or relative weight.
  pub fn weight(&self) -> Result<CpuWeight, Error> {
    self.cgroup.parse("cpu.weight")
  }

  /// Configuration snapshot: coarse nice representation of weight.
  pub fn weight_nice(&self) -> Result<Nice, Error> {
    self.cgroup.parse("cpu.weight.nice")
  }

  /// Configuration snapshot: this cgroup's quota and period, not ancestor limits.
  pub fn max(&self) -> Result<CpuMax, Error> {
    self.cgroup.parse("cpu.max")
  }

  /// Configuration snapshot: CPU burst allowance.
  pub fn max_burst(&self) -> Result<CpuMaxBurst, Error> {
    self.cgroup.parse("cpu.max.burst")
  }

  /// Live PSI averages and totals. Never registers a pressure trigger.
  pub fn pressure(&self) -> Result<Pressure, Error> {
    self.cgroup.parse("cpu.pressure")
  }

  /// Configuration snapshot: requested utilization floor, not guaranteed bandwidth.
  pub fn uclamp_min(&self) -> Result<CpuUclampMin, Error> {
    self.cgroup.parse("cpu.uclamp.min")
  }

  /// Configuration snapshot: requested utilization ceiling.
  pub fn uclamp_max(&self) -> Result<CpuUclampMax, Error> {
    self.cgroup.parse("cpu.uclamp.max")
  }

  /// Configuration snapshot: whether the cgroup uses idle scheduling.
  pub fn idle(&self) -> Result<CpuIdle, Error> {
    self.cgroup.parse("cpu.idle")
  }
}
