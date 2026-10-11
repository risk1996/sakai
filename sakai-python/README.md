# Sakai Python bindings

`sakai` exposes the current read-only Linux cgroup v2 readers from
the `sakai` Rust crate. Python 3.11 or newer is required. The native module
uses the `cp311-abi3` stable ABI for regular CPython; free-threaded CPython
needs a separate wheel and is not supported by this build.

```python
from sakai import Cgroup, InterfaceMissingError

group = Cgroup.current()
cpu_time_ns = group.cpu().stat().time.usage_ns
quota = group.cpu().max()
print(cpu_time_ns, "max" if quota.cpu_count.is_max else quota.cpu_count.value)

try:
  memory_bytes = group.memory().current()
except InterfaceMissingError as error:
  print(f"Unavailable interface: {error.path}")
```

Every reader method opens a fresh read-only kernel interface. Readings from separate
methods are not atomic together. Limits use `MaxOr[int]` or `MaxOr[float]`:
check `.is_max` before reading `.value`, which raises `ValueError` for `max`.
These limits do not account for ancestor limits, affinity, or other policy. The `path`
property is for display and may become stale; live reads continue through the
original open directory handle.

Compound readings are immutable native snapshot objects. Times are integer
nanoseconds, memory amounts are integer bytes, page counts stay integers in
pages, and event counts are integers. Pressure averages and utilization clamps
are floating-point ratios from zero to one. Optional counters use `None` when
omitted by the kernel, preserving the distinction from zero.

`Cgroup.from_path()` and `.child()` accept `str`, `bytes`, and `os.PathLike`,
including non-UTF-8 names. Child names must be one direct filesystem component;
the core rejects traversal and slashes. Reader objects retain the open handle
after the original `Cgroup` object is released; `.children()` returns separate
pinned handles.

`SakaiError` is the base for `InterfaceMissingError`, `NotCgroupV2Error`,
`DeletedCgroupError`, `CgroupParseError`, and `NotSupportedError`. Missing-interface,
deleted-cgroup, and parse exceptions carry a diagnostic `.path`; parse messages
include the underlying failure. Other I/O failures retain standard `OSError`
subclasses and `.errno`, including `FileNotFoundError` for a missing directory.

`group.memory().events()` reads hierarchical counters (local on mounts with
`memory_localevents`); `.events_local()` excludes descendants. Both return
immutable `MemoryEventCounts`, including optional `.oom_group_kill` and
`.sock_throttled` counters. `.oom_group()` returns the boolean group OOM policy.

`group.memory().swap()` reads swap usage, peak usage, limits, and event counters.
`group.memory().zswap()` reads compressed pool usage, its limit, and the configured
disk writeback policy. Both readers retain the pinned handle independently.
Byte limits use `MaxOr[int]`; swap event counters are integers, with `.high`
set to `None` when an older kernel omits that counter. Missing files raise
`InterfaceMissingError` with the interface path.

`group.memory().numa_stat()` groups statistics into `.bytes`, `.pages`, and
`.counts`, like `memory.stat()`. Each mapping contains kernel field names mapped
to NUMA node IDs and values, for example `numa.bytes["anon"][0]`. Both mapping
levels are read-only, and unknown fields are ignored.

`group.pids()` reads `.current()` as an integer task count, `.max()` as
`MaxOr[int]`, and `.events().max` as an integer limit-event count. Task counts
include threads and can exceed the configured limit. Event counts normally
include descendants; older kernels and `pids_localevents` report local events.
`group.core().events()` returns immutable `.populated` and `.frozen` states;
`.frozen` is `None` when an older kernel omits it. `group.core().stat()` returns
integer `.descendants` and `.dying_descendants` counts and read-only `.subsystems`
and `.dying_subsystems` mappings from controller names to integer object counts.
Missing subsystem counters are absent from the maps. Every reading is volatile;
missing files, including root exemptions, raise `InterfaceMissingError`.

`group.io()` exposes `.stat()`, `.pressure()`, `.weight()`, `.max()`, and
`.latency()`. I/O snapshots use read-only device mappings keyed by `(major, minor)`
integer tuples. `.stat().devices` contains immutable byte and operation counters;
missing counters remain `None`. `.weight()` exposes `.default_weight` and numeric
`.devices` overrides. `.max().devices` contains `MaxOr[int]` limits named
`.read_bps`, `.write_bps` (bytes per second), `.read_iops`, and `.write_iops`
(operations per second). `.latency().targets_ns` maps configured devices to
nonzero nanosecond targets; disabled targets are omitted. Device maps can be
empty, and every read is a separate snapshot. Optional files and root exemptions
raise `InterfaceMissingError` with their interface path. Pressure uses the same
`Pressure` snapshot as CPU and memory, and reading it never writes a PSI trigger.

`group.cpuset()` provides `.cpus()`, `.cpus_effective()`, `.mems()`,
`.mems_effective()`, and `.partition()`. CPU and memory-node snapshots use
`CpuList` and `MemoryNodeList`, with immutable `.ranges` tuples of inclusive
`(start, end)` ID pairs and `.contains(id)` membership checks. IDs are unsigned
32-bit identifiers; large ranges stay compact. Empty configured lists mean
inheritance, while empty effective lists mean no grant. Effective placement can
change with hotplug and ancestor configuration. `CpusetPartition` exposes
`.state` (`member`, `root`, or `isolated`), `.is_invalid`, and `.reason`, preserving
older invalid states without reasons as `None`. Missing files, including root
configuration exemptions, raise `InterfaceMissingError`. Every call reads fresh
values through a retained pinned handle; reads never migrate memory or change
placement. See the [kernel cpuset documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).

Native classes, methods, and snapshot attributes carry Python `__doc__`
strings generated from their Rust documentation comments. Use `help(Cgroup)`
or inspect an individual method such as `Cgroup.current.__doc__`.

Ruff and Ty are supplied by the devenv Python profile. The Python sources use
two-space indentation and the repository's `ruff.toml` enables Ruff's `ALL`
rule set with narrow project-specific exceptions. Run lint, formatting, and
Python 3.11 type checks with:

```text
devenv --profile python test
```

The devenv Python profile uses Python 3.15, and CI tests installed wheels and
source distributions on Python 3.11 and 3.15.

The package version is inherited from Cargo's workspace release version via
Maturin's dynamic version metadata. The devenv Python interpreter is a development
tool choice; it does not change the package's minimum supported Python version.

To build locally on Linux, use an explicit Python 3.11 or newer interpreter:

```text
devenv --profile python shell -- rtk maturin build --manifest-path sakai-python/Cargo.toml --interpreter /path/to/python3.11 --locked
devenv --profile python shell -- rtk maturin sdist --manifest-path sakai-python/Cargo.toml
```

Install the wheel into a clean environment and run
`python -m unittest discover -s sakai-python/tests` from outside the checkout.
Also rebuild and install the sdist in a separate clean environment: its path
dependency on `../sakai` needs the included Rust crate, workspace manifests,
and lockfile. CI performs both installed-package checks and verifies the
`cp311-abi3` wheel tag. `devenv --profile python test` runs Rust, lint, and type
checks; the Linux package contract suite runs in the separate CI packaging job.

Publication decisions and artifact verification remain in the
[PyPI pre-publication checklist](../design/pypi-prepublish-checklist.md).
Adding free-threaded wheels requires a separate ABI choice, native-state audit,
and build/test matrix; adding portable parser bindings requires a concrete use
case. Neither is part of this Linux handle package.

For binding mechanics, see the versioned [PyO3 guide](https://pyo3.rs/v0.29.2/),
especially [classes](https://pyo3.rs/v0.29.2/class),
[exceptions](https://pyo3.rs/v0.29.2/exception.html),
[parallelism](https://pyo3.rs/v0.29.2/parallelism), and
[free-threading](https://pyo3.rs/v0.29.2/free-threading.html), plus
[Maturin local development](https://www.maturin.rs/local_development).
The [Python binary-extension guide](https://packaging.python.org/en/latest/guides/packaging-binary-extensions/)
and [package-format guide](https://packaging.python.org/en/latest/discussions/package-formats/)
explain ABI and wheel portability. Public naming, units, and exception policy
are defined by this package's native docstrings, stubs, and contract tests.

For exact assertions against Kubernetes container CPU and memory resources,
run the [Kubernetes resource test](../tools/kubernetes/README.md). It checks the
installed Python bindings inside a Pod, including request mappings with
MemoryQoS enabled.
