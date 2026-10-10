# Sakai

A _read-only_ Rust cgroup v2 library. While kernel reads require Linux, the
parsers are OS-independent. Python bindings are work in progress.

```rust,no_run
use sakai::{Cgroup, Error};

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

`cgroup.pids()` reads the current hierarchical task count, configured limit,
and process-limit events. Counts include threads; current usage can exceed
the limit after migration or a limit reduction. `cgroup.core().events()` reads
populated and completed frozen state, preserving a missing older-kernel frozen
field as `None`. `cgroup.core().stat()` reads descendant counts and maps of live
and dying subsystem counts keyed by controller. Older kernels may omit those
maps; absent counters do not imply zero. These states and counts are volatile.
The interfaces follow the [kernel cgroup v2 documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html).

Times use `u64` nanoseconds with checked conversion from kernel microseconds.
Counts have a separate `uom` kind. Ratios use `f64` to preserve fractional
percentages. Integer quantity arithmetic truncates; use fractional operands
when calculating rates. `CpuMax::cpu_count()` is only this cgroup's quota/period
ratio: it does not resolve affinity, ancestor quotas, or scheduling policy.

Shared values are available as `sakai::{Time, Count, EventRate, Ratio,
NonZeroTime, MaxOr, Pressure, PressureLine}`. Parse failures use
`sakai::error::{ParseError, ParseValueError, FieldKind}`. Linux read errors
include the full interface path and retain the parser error as their source.

Run `devenv test` for formatting, Clippy, parser tests, and doctests.
On Linux this also runs an unprivileged read of the current cgroup.
It also checks Rust dependency licenses with `cargo-license`, installed through
devenv. Run `devenv tasks run check:licenses` for the license check alone.
The check covers all workspace members and features, including transitive,
build, dev, and target-specific dependencies. Cargo.lock must remain unchanged.

The dependency policy accepts explicitly reviewed SPDX expressions that offer
permissive terms: MIT, Apache-2.0 (including LLVM-exception), BSD-3-Clause,
BSL-1.0, ISC, Zlib, Unlicense, Unicode-3.0, and CDLA-Permissive-2.0.
For dual licenses with GPL/LGPL alternatives, select the MIT or Apache terms;
all terms joined by `AND` must be acceptable. The exact reviewed expressions
are maintained in `xtask/src/licenses.rs`. New expressions, missing license
metadata (including license-file-only crates), and malformed reports fail CI
and require review. This checks declared Cargo metadata; distribution must
still preserve the chosen licenses' notices and other requirements.

`devenv shell -- rtk cargo xtask vmtest` runs the isolated Linux VM suite;
only its explicit delegated-cgroup fixture writes test configuration.
CI tests macOS and Linux, plus multiple Linux kernel versions.
