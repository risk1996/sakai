# Sakai

A _read-only_ Rust cgroup v2 library. While kernel reads require Linux, the
parsers are OS-independent. Python bindings are work in progress.

```rust,no_run
use sakai_core::{Cgroup, Error};

fn main() -> Result<(), Error> {
  let cgroup = Cgroup::from_current_process()?;
  let stat = cgroup.cpu().stat()?;
  let quota = cgroup.cpu().max()?;
  println!("usage: {:?}, quota: {:?}", stat.time().usage(), quota);
  Ok(())
}
```

`Cgroup` pins an open directory and verifies the cgroup2 filesystem. Discovery
uses procfs membership and the caller's mount namespace, including bind mounts.
It does not assume `/sys/fs/cgroup` is a unified hierarchy. Legacy-only hosts
return `NotCgroupV2`; hybrid hosts may expose a v2 hierarchy without CPU controls.
Missing interfaces return `Error::FileMissing`, including root exemptions and
files unavailable on older kernels. Other errors retain their cause.

Every call reads fresh contents. Multiple reads are not an atomic snapshot.
`cpu.stat` usage includes descendants; its bandwidth counters describe its own
limit. `cpu.stat.local` describes local runqueue throttling, including ancestor
limits. Unknown CPU stat counters are ignored.

Times use `u64` nanoseconds with checked conversion from kernel microseconds.
Counts have a separate `uom` kind. Ratios use `f64` to preserve fractional
percentages. Integer quantity arithmetic truncates; use fractional operands
when calculating rates. `CpuMax::cpu_count()` is only this cgroup's quota/period
ratio: it does not resolve affinity, ancestor quotas, or scheduling policy.

Shared values are available as `sakai_core::{Time, Count, EventRate, Ratio,
NonZeroTime, MaxOr, Pressure, PressureLine}`. Parse failures use
`sakai_core::error::{ParseError, ParseValueError, FieldKind}`. Linux read errors
include the full interface path and retain the parser error as their source.

Run `devenv test` for formatting, Clippy, parser tests, and doctests.
On Linux this also runs an unprivileged read of the current cgroup.
`devenv shell -- rtk cargo xtask vmtest` runs the isolated Linux VM suite;
only its explicit delegated-cgroup fixture writes test configuration.
CI tests macOS and Linux, plus multiple Linux kernel versions.
