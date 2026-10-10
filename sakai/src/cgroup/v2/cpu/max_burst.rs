use crate::{
  scalar::{Scalar, interface},
  unit::Time,
};

/// The CPU bandwidth burst allowance configured by a cgroup's
/// `cpu.max.burst` file.
///
/// A zero duration disables bursting.
pub type CpuMaxBurst = Scalar<Time, interface::CpuMaxBurst>;
