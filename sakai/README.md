# Sakai

Linux cgroup v2 metrics and configuration, with portable parsers and typed values.
Rust 1.88 or newer is required. Linux 5.15 is the oldest kernel in the test
matrix; newer interfaces depend on kernel version and configuration.

## Installation

```toml
[dependencies]
sakai = "0.1.0-alpha.1"
```

## Read a cgroup on Linux

```rust,no_run
#[cfg(target_os = "linux")]
fn main() -> Result<(), sakai::Error> {
  let cgroup = sakai::Cgroup::from_current_process()?;
  let stat = cgroup.cpu().stat()?;
  let quota = cgroup.cpu().max()?;
  println!("usage: {:?}, quota: {:?}", stat.time().usage(), quota);
  Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {}
```

CPU, memory, task-count, core, I/O, cpuset, HugeTLB, RDMA, device-memory, and
miscellaneous-resource readers open fresh read-only
kernel interfaces. Each result is an owned snapshot. Getters inspect the stored
snapshot; reading separate files does not produce an atomic snapshot.

Times use nanoseconds, memory uses bytes, and counts have a separate quantity
type. For limit interfaces, `MaxOr::Max` means no limit at this cgroup, while
ancestors may still impose limits. Other interfaces define the token's meaning
separately; device-memory usage preserves a kernel counter sentinel.
`CpuMax::cpu_count()` is the quota/period ratio; it does not resolve
affinity, cpuset placement, ancestor limits, or scheduling policy.

Discovery uses procfs membership and the caller's mount namespace instead of
assuming `/sys/fs/cgroup` is the active hierarchy. Missing or disabled interfaces,
including hierarchy-root exemptions, return `Error::FileMissing`. Optional
older-kernel counters remain `None`, distinct from measured zero. Readers never
enable controllers, change placement, or write pressure triggers.

## Change a cgroup configuration on Linux

Edit the returned value and pass it to the controller setter. For example,
this replaces both `cpu.max` fields in a cgroup you can write to.

```rust,no_run
#[cfg(target_os = "linux")]
fn set_cpu_bandwidth(cgroup: &sakai::Cgroup) -> Result<(), Box<dyn std::error::Error>> {
  use sakai::{MaxOr, NonZeroTime, Time};
  use uom::si::time::microsecond;

  let cpu = cgroup.cpu();
  let mut limit = cpu.max()?;
  limit.quota = MaxOr::Value(NonZeroTime::try_new(Time::new::<microsecond>(30_000))?);
  limit.period = NonZeroTime::try_new(Time::new::<microsecond>(50_000))?;
  cpu.set_max(&limit)?;
  Ok(())
}
```

Each setter submits one kernel command. Another writer can change settings
between the read and write. Use `Cpu::set_quota` to change the quota while
the kernel retains the current period. For keyed limits such as `io.max`,
each command applies to one device.

## Parse saved contents on any platform

```rust
use sakai::{MaxOr, v2::cpu::CpuMax};

let quota: CpuMax = "25000 100000".parse()?;
assert!(matches!(quota.cpu_count(), MaxOr::Value(_)));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Linux handles are gated by the operating system; snapshot types and their
`FromStr` parsers remain available on other platforms.

See the [API documentation](https://docs.rs/sakai),
[source repository](https://github.com/risk1996/sakai), and
[changelog](CHANGELOG.md). The interface semantics follow the
[Linux cgroup v2 documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html)
and [Linux pressure stall information](https://docs.kernel.org/accounting/psi.html).

Sakai is licensed under MIT.
