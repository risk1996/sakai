use crate::scalar::{Scalar, interface};

/// The idle scheduling state configured by a cgroup's `cpu.idle` file.
///
/// `true` (kernel `1`) selects idle scheduling for the cgroup; `false` (kernel
/// `0`) selects normal weight-based scheduling. The fair-class scheduler
/// gives idle groups very low priority relative to their peers. A BPF
/// scheduler can also apply this setting through its idle callback.
/// The associated `cpu.weight` read reports `0` while idle scheduling is enabled.
///
/// See [Linux `cpu.idle` interface](https://www.kernel.org/doc/html/v7.3/admin-guide/cgroup-v2.html#cpu-interface-files).
pub type CpuIdle = Scalar<bool, interface::CpuIdle>;
