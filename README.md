# Sakai

A _read-only_ Rust cgroup v2 library. While kernel reads require Linux, the
parsers are OS-independent. Python bindings are work in progress.

The `sakai` crate supports Rust 1.88 and newer. This is a fixed minimum,
not a policy that tracks the latest stable release. Raising it requires an
explicit, documented change. Development tools and Python bindings use their
own toolchain requirements. CI tests the crate on the minimum and stable Rust
versions on Linux and macOS.

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
`devenv shell -- rtk cargo xtask vmtest` runs the isolated Linux VM suite;
only its explicit delegated-cgroup fixture writes test configuration.
CI tests macOS and Linux, plus multiple Linux kernel versions.

## Dependency security

Run `devenv tasks run check:security` to audit `Cargo.lock` against the current
[RustSec advisory database](https://rustsec.org/) and check the dependency policy
in `deny.toml`. Run `check:audit` or `check:deny` individually for either check.
These checks require network access and are separate from `devenv test`.
CI runs both on pull requests, pushes to `main`, manual runs, and every Monday
at 03:17 UTC (12:17 JST), including when dependencies have not changed.

Vulnerabilities, unsoundness, maintenance notices, unmaintained crates, and yanked
versions fail the security checks, including transitive and test dependencies.
Dependency licenses must be explicitly allowed, and dependency sources must be
crates.io. Duplicate crate versions produce warnings for review.

Upgrade affected dependencies first. If an advisory cannot be fixed immediately,
document why it does not affect Sakai and when the exception will be reviewed
before adding a specific advisory ID to `.cargo/audit.toml` and `deny.toml`.
Do not suppress a class of advisories or lower the severity threshold.
Dependabot already checks Cargo dependencies and GitHub Actions weekly;
repository administrators should also enable its security alerts and security
updates in GitHub's repository settings.

For production executables that use Sakai, RustSec recommends embedding the
dependency tree with `cargo auditable build --release --locked`, then scanning
the resulting executable with `cargo audit bin <path>`. Both tools are available
in the development shell. Sakai itself ships a Rust library and Python extension,
so it has no production executable build to wrap with `cargo auditable`.
