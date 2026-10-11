# Sakai

Read-only Linux cgroup v2 metrics, with portable parsers and typed snapshots.
Rust 1.88 or newer is required. Linux 5.15 is the oldest kernel in the test
matrix; newer interfaces depend on kernel version and configuration.

## Installation

```toml
[dependencies]
sakai = "0.1"
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

CPU, memory, process-count, core, I/O, and cpuset readers open fresh read-only
kernel interfaces. Each result is an owned snapshot. Getters inspect the stored
snapshot; reading separate files does not produce an atomic snapshot.

Times use nanoseconds, memory uses bytes, and counts have a separate quantity
type. `MaxOr::Max` means no limit at this cgroup, while ancestors may still impose
limits. `CpuMax::cpu_count()` is the quota/period ratio; it does not resolve
affinity, cpuset placement, ancestor limits, or scheduling policy.

Discovery uses procfs membership and the caller's mount namespace instead of
assuming `/sys/fs/cgroup` is the active hierarchy. Missing or disabled interfaces,
including hierarchy-root exemptions, return `Error::FileMissing`. Optional
older-kernel counters remain `None`, distinct from measured zero. Readers never
enable controllers, change placement, or write pressure triggers.

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
