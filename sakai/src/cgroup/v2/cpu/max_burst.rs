use crate::{
  scalar::{Scalar, interface},
  unit::Time,
};

/// The CPU bandwidth burst allowance configured by a cgroup's
/// `cpu.max.burst` file.
///
/// Unused quota can be saved for later bursts up to this allowance, measured
/// in kernel microseconds and exposed as [`Time`]. A zero duration disables
/// bursting. This is CPU time, rather than a frequency or a wall-clock rate.
/// See [Linux `cpu.max.burst` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
///
/// Since Linux 5.16; see the [upstream Linux 5.16 source](https://github.com/torvalds/linux/blob/v5.16/Documentation/admin-guide/cgroup-v2.rst).
/// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
pub type CpuMaxBurst = Scalar<Time, interface::CpuMaxBurst>;
