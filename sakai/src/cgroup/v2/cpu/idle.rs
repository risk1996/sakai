use crate::scalar::{Scalar, interface};

/// The idle scheduling state configured by a cgroup's `cpu.idle` file.
///
/// `true` (kernel `1`) selects idle scheduling for the cgroup; `false` (kernel
/// `0`) selects normal weight-based scheduling. Idle groups receive CPU time
/// only when competing non-idle groups do not need it. The associated
/// `cpu.weight` read reports `0` while idle scheduling is enabled.
/// See [Linux `cpu.idle` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
pub type CpuIdle = Scalar<bool, interface::CpuIdle>;
