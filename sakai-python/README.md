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

`group.hugetlb()` reads huge-page interfaces for a canonical binary page-size
label such as `"2MB"` or `"1GB"`. These are HugeTLB pages, separate from
transparent huge pages. `.current(size)` and `.reserved_current(size)` return
bytes; `.max(size)` and `.reserved_max(size)` return `MaxOr[int]` byte limits.
Reservation accounting includes reserved pages and faults without prior
reservations, so it does not measure unused reservations alone. `.events(size)`
and `.events_local(size)` return immutable `.max` failure counts with hierarchical
and local scope. `.numa_stat(size)` reports byte `.total` and a read-only `.nodes`
mapping from NUMA IDs to byte usage; it requires Linux 5.17. Invalid or
noncanonical labels raise `ValueError`; unavailable sizes and root interfaces
raise `InterfaceMissingError`. See [Linux HugeTLB interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#hugetlb).

`group.rdma()` reads `.current()` and `.peak()` as immutable `RdmaCurrent`
snapshots with read-only `.devices` mappings keyed by `(name, index)` tuples.
The index is an unsigned device identifier, or `None` when the kernel omits it;
`None` differs from zero. Linux 7.3 release candidates add an index for names
shared across network namespaces; older kernels and unique names omit it.
For example, `snapshot.devices[("mlx4_0", None)]` accesses a name-only record,
while `snapshot.devices[("rxe0", 5)]` selects an indexed record. Name collisions,
renames and index reuse can change keys between snapshots. See the
[Linux 7.3-rc1 implementation](https://github.com/torvalds/linux/blob/v7.3-rc1/kernel/cgroup/rdma.c).
Each device exposes integer `.hca_handle` and `.hca_object` resource counts.
`.max()` has the same device keys and `MaxOr[int]` resource limits. Device pools
may disappear when resources are released; an empty map is valid. `.events()`
and `.events_local()` map devices to `.hca_handle` and `.hca_object` event objects,
each with integer `.max` and `.alloc_fail` counters. Peak and event interfaces
require Linux 7.2; local max counts hits at this cgroup’s own limit, while local
alloc_fail counts failed requests originating here, including ancestor limits.
These interfaces are absent at the root; missing files on older kernels raise
`InterfaceMissingError`.
See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma).

`group.dmem()` provides `.capacity()` and `.current()` byte snapshots with
read-only `.regions` mappings keyed by driver-defined device-memory region names.
`.min()`, `.low()`, and `.max()` return `DmemLimits`, whose `.regions` values are
`MaxOr[int]` bytes. Min and low describe hard and best-effort protection; max is
the hard allocation limit. This controller requires Linux 6.14, its kernel
configuration and a registering device driver. Capacity exists only at the root;
protection and limit files exist only below it. Missing files raise
`InterfaceMissingError`, while an empty map means no regions were reported.
See [Linux device-memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#dmem).

`group.misc()` reads `.capacity()`, `.current()`, and `.peak()` with immutable
`.resources` mappings from hardware-dependent resource names, such as `sev`, to
integer resource counts. These values are counts, not bytes. `.max()` maps the
same names to `MaxOr[int]` resource limits; limits can exceed machine capacity.
`.events()` and `.events_local()` map resource names to max-boundary failure
counts. Hierarchical events aggregate limit-hit locations in the subtree; local
events count failures at this cgroup's own limit, including descendant requests.
A request rejected at an ancestor's limit is counted there. Capacity is root-only;
max and events are non-root. Events require Linux 5.16, root current requires
Linux 6.5, and peak and local events require Linux 6.11. Empty files are valid;
omitted resource names remain absent, distinct from a measured zero.
See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).

All four readers retain the pinned handle, perform fresh read-only I/O, and
return separate, volatile snapshots. Mapping properties and nested snapshot
objects are immutable; reading them performs no I/O. Ancestor limits and
hardware capacity can further constrain allocations. Controller availability,
kernel configuration, versions and root exemptions can omit files; the exception
policy above applies to every reader. These readers never allocate resources,
change configuration or reset peaks.

Native classes, methods, and snapshot attributes carry Python `__doc__`
strings generated from their Rust documentation comments. Use `help(Cgroup)`
or inspect an individual method such as `Cgroup.current.__doc__`.

`group.poll_events(filename)` opens an `EventPoller` for `cgroup.events`,
`memory.events`, `memory.events.local`, `memory.swap.events`, `pids.events`,
or `cpuset.cpus.partition`. Opening acknowledges the current state without
parsing or returning it. `.wait(timeout)` returns `True` when an event hint is
acknowledged and `False` on timeout. Read through the existing reader after
each hint to obtain a fresh snapshot; notifications can coalesce, and they
do not count changes. Memory and process event scope remains the scope of
the selected file and mount options. Polling never writes a file. PSI pressure
files are excluded because installing their threshold triggers requires writes.
See the [kernel event interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html)
and [poll manual](https://man7.org/linux/man-pages/man2/poll.2.html).

```python
events = group.poll_events("memory.events")
previous = group.memory().events()
if events.wait(timeout=1.0):
  current = group.memory().events()
  print(previous.oom_kill, current.oom_kill)
```

`group.watch_configuration(filename)` opens a `ConfigurationWatcher` using
inotify for one of these configuration files:

| Controller | Supported filenames |
| --- | --- |
| Core | `cgroup.type`, `cgroup.subtree_control` |
| CPU | `cpu.max`, `cpu.weight`, `cpu.uclamp.max`, `cpu.uclamp.min`, `cpu.max.burst`, `cpu.weight.nice`, `cpu.idle` |
| Memory | `memory.max`, `memory.high`, `memory.low`, `memory.min`, `memory.oom.group`, `memory.swap.max`, `memory.swap.high`, `memory.zswap.max`, `memory.zswap.writeback` |
| Processes | `pids.max` |
| I/O | `io.weight`, `io.max`, `io.latency` |
| Placement | `cpuset.cpus`, `cpuset.mems`, `cpuset.cpus.partition` |

`.wait(timeout)` returns an immutable `ChangeNotification` or `None` on
timeout. `.modified` reports write or kernel content hints; `.removed` reports
watch termination through removal, unmount, or invalidation;
`.overflowed` reports lost inotify events. Flags can overlap. Read configuration
again after modification, rescan after overflow, and recreate the watcher after
removal if a replacement exists. Subsequent waits on a removed watch raise
`BrokenPipeError`. The watch follows its inode through renames and does not
follow replacements. Cgroup interface deletion does not guarantee an inotify
event: a watch can remain idle after the interface disappears. Use bounded waits
and fresh reader calls to check lifecycle; an indefinite wait may remain blocked
after deletion. See the [kernfs removal implementation](https://github.com/torvalds/linux/blob/v5.15/fs/kernfs/dir.c#L1236-L1295).
Inotify watches observable file activity; kernel-driven
configuration changes that emit no fsnotify event are not guaranteed to appear.
It does not monitor usage counters, descendants, or effective ancestor policy.
See the [inotify manual](https://man7.org/linux/man-pages/man7/inotify.7.html).

```python
configuration = group.watch_configuration("cpu.max")
change = configuration.wait(timeout=1.0)
if change is not None:
  if change.removed:
    print("The watch ended")
  elif change.modified or change.overflowed:
    print(group.cpu().max().cpu_count)
```

Both monitor types own their descriptors after the original `Cgroup` is
released and close them when the monitor object is released. They release the
GIL while waiting. Timeouts use seconds: `None` waits indefinitely, `0` checks
immediately, and finite nonnegative values bound the kernel wait. Negative,
nonfinite, or unrepresentably large values raise `ValueError`. A concurrent
wait on the same object raises `RuntimeError`; separate monitors can wait
independently. Unknown filenames and paths raise `ValueError`; unavailable
files, including root exemptions and unsupported optional controller files,
raise `InterfaceMissingError` with their diagnostic path. Monitor errors use
the same exception policy as readers. Event polling on a deleted cgroup can
fail while acknowledging the interface, reporting an I/O or Sakai error.

For an external event loop, `monitor.fileno()` borrows its descriptor. Keep
the monitor alive and do not close or read the descriptor yourself. Register
an `EventPoller` with `select.poll()` or epoll for **`POLLPRI | POLLERR`**;
ordinary readable readiness on a cgroup event file is continuously true.
Register a `ConfigurationWatcher` for ordinary readable readiness (`POLLIN`),
which also works with `select.select()` and `selectors`. After readiness,
call `.wait(0)` to acknowledge or consume hints, then perform fresh reads.

```python
import select

poll = select.poll()
poll.register(events, select.POLLPRI | select.POLLERR)
if poll.poll(1000) and events.wait(0):  # select.poll's timeout is milliseconds
  print(group.memory().events().oom_kill)
```

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
Maturin's dynamic version metadata, which converts Rust's `0.1.0-alpha.1` to
Python's `0.1.0a1`. The devenv Python interpreter is a development
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
