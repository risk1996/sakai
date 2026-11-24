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
use crate::error::Error;
use crate::pressure::Pressure;

mod idle;
mod max;
mod max_burst;
mod stat;
mod uclamp;
mod weight;
mod weight_nice;

/// A borrowed view of one open cgroup's CPU interfaces.
///
/// Each read returns a fresh snapshot through the cgroup's directory. Missing
/// optional interfaces return [`Error::FileMissing`]; separate reads are not
/// atomic together.
///
/// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Cpu<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Cpu<'_> {
  /// Sets the CPU scheduling weight. Use [`Self::set_idle`] to change idle mode.
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_weight(&self, weight: Weight) -> Result<(), Error> { self.cgroup.write_value::<CpuWeight>(weight) }

  /// Sets the CPU scheduling weight using a nice value.
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_weight_nice(&self, nice: Nice) -> Result<(), Error> { self.cgroup.write_value::<Nice>(&nice) }

  /// Updates the CPU quota while the kernel retains the current period.
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// # Errors
  /// A finite quota must be an exact whole number of microseconds. See the shared
  /// [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_quota(&self, quota: crate::MaxOr<crate::NonZeroTime>) -> Result<(), Error> {
    self.cgroup.write_value::<CpuQuota>(quota)
  }

  /// Live accounting, including descendants; available with the controller off.
  ///
  /// Reads a fresh [`CpuStat`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn stat(&self) -> Result<CpuStat, Error> { self.cgroup.read_value::<CpuStat>() }

  /// Live local runqueue throttling, including inherited bandwidth limits.
  ///
  /// Reads a fresh [`CpuStatLocal`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Since Linux 6.6; see the [upstream Linux 6.6 source](https://github.com/torvalds/linux/blob/v6.6/kernel/cgroup/cgroup.c).
  /// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn stat_local(&self) -> Result<CpuStatLocal, Error> { self.cgroup.read_value::<CpuStatLocal>() }

  /// Configuration snapshot: idle scheduling or relative weight.
  ///
  /// Reads a fresh [`CpuWeight`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn weight(&self) -> Result<CpuWeight, Error> { self.cgroup.read_value::<CpuWeight>() }

  /// Configuration snapshot: coarse nice representation of weight.
  ///
  /// Reads a fresh [`Nice`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn weight_nice(&self) -> Result<Nice, Error> { self.cgroup.read_value::<Nice>() }

  /// Configuration snapshot: this cgroup's quota and period, not ancestor limits.
  ///
  /// Reads a fresh [`CpuMax`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max(&self) -> Result<CpuMax, Error> { self.cgroup.read_value::<CpuMax>() }

  /// Replaces the CPU quota and period together in one write.
  ///
  /// Edit the public fields of a value returned by [`Self::max`], then pass
  /// that value here. Both fields are sent even if only one field was edited.
  /// A concurrent writer can change the configuration between read and write.
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// # Errors
  /// Both durations must be exact whole microseconds. See the shared
  /// [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_max(&self, value: &CpuMax) -> Result<(), Error> { self.cgroup.write_value::<CpuMax>(value) }

  /// Configuration snapshot: CPU burst allowance.
  ///
  /// Reads a fresh [`CpuMaxBurst`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Since Linux 5.16; see the [upstream Linux 5.16 source](https://github.com/torvalds/linux/blob/v5.16/Documentation/admin-guide/cgroup-v2.rst).
  /// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max_burst(&self) -> Result<CpuMaxBurst, Error> { self.cgroup.read_value::<CpuMaxBurst>() }

  /// Sets the CPU burst allowance as a duration in whole microseconds.
  /// See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_max_burst(&self, value: crate::Time) -> Result<(), Error> { self.cgroup.write_value::<CpuMaxBurst>(value) }

  /// Live PSI averages and totals. Never registers a pressure trigger.
  ///
  /// Reads a fresh [`Pressure`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn pressure(&self) -> Result<Pressure, Error> { self.cgroup.read_value::<CpuPressure>() }

  /// Configuration snapshot: requested utilization floor, not guaranteed bandwidth.
  ///
  /// Reads a fresh [`CpuUclampMin`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn uclamp_min(&self) -> Result<CpuUclampMin, Error> { self.cgroup.read_value::<CpuUclampMin>() }

  /// Sets the minimum CPU utilization clamp as a ratio from zero to one.
  /// See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_uclamp_min(&self, value: crate::Ratio) -> Result<(), Error> {
    self.cgroup.write_value::<CpuUclampMin>(value)
  }

  /// Configuration snapshot: requested utilization ceiling.
  ///
  /// Reads a fresh [`CpuUclampMax`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn uclamp_max(&self) -> Result<CpuUclampMax, Error> { self.cgroup.read_value::<CpuUclampMax>() }

  /// Sets the maximum CPU utilization clamp as a ratio from zero to one, or removes the cap with `max`.
  /// See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_uclamp_max(&self, value: crate::MaxOr<crate::Ratio>) -> Result<(), Error> {
    self.cgroup.write_value::<CpuUclampMax>(value)
  }

  /// Configuration snapshot: whether the cgroup uses idle scheduling.
  ///
  /// Reads a fresh [`CpuIdle`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn idle(&self) -> Result<CpuIdle, Error> { self.cgroup.read_value::<CpuIdle>() }

  /// Sets whether this cgroup uses idle CPU scheduling.
  /// See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_idle(&self, value: bool) -> Result<(), Error> { self.cgroup.write_value::<CpuIdle>(value) }
}

crate::v2::interface::cgroup_file!(alias
  /// CPU pressure snapshots.
  CpuPressure, Pressure::CPU_FILE_NAME, Read, Pressure
);
