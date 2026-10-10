use crate::scalar::{Scalar, interface};

/// The idle scheduling state configured by a cgroup's `cpu.idle` file.
///
/// When enabled, the cgroup uses the idle scheduling policy.
pub type CpuIdle = Scalar<bool, interface::CpuIdle>;
