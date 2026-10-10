use crate::{
  limit::MaxOr,
  scalar::{Scalar, interface},
  unit::Ratio,
};

/// The minimum CPU utilization requested by a cgroup's `cpu.uclamp.min` file.
///
/// The kernel percentage is exposed as a dimensionless [`Ratio`]. This is a
/// performance hint rather than a CPU bandwidth guarantee.
pub type CpuUclampMin = Scalar<Ratio, interface::CpuUclampMin>;

/// The maximum CPU utilization requested by a cgroup's `cpu.uclamp.max` file.
///
/// A concrete kernel percentage is exposed as a dimensionless [`Ratio`].
/// [`MaxOr::Max`] requests no cap. This is a performance hint rather than a
/// CPU bandwidth limit.
pub type CpuUclampMax = Scalar<MaxOr<Ratio>, interface::CpuUclampMax>;
