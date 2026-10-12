# Linux cgroup v2 implementation audit

Reviewed on 2026-10-12 against the [kernel cgroup v2 manual](https://docs.kernel.org/admin-guide/cgroup-v2.html),
[PSI manual](https://docs.kernel.org/accounting/psi.html), and upstream implementation
tables and output routines. Introduction versions were checked against adjacent
release-tagged sources, rather than inferred from current documentation.
Linux 5.15 remains the oldest kernel in the existing test matrix.

## Corrected semantics

| Interface | Finding and correction | Upstream evidence |
| --- | --- | --- |
| `memory.min`, `memory.low` | Both accept and print `max`. Rust now preserves `MaxOr<Bytes>`; Python returns `MaxOr[int]`. | [memory output routines](https://github.com/torvalds/linux/blob/v6.18/mm/memcontrol.c) |
| `dmem.current`, `dmem.peak` | Their shared output routine prints `max` at `PAGE_COUNTER_MAX`. Preserve the sentinel without guessing a byte value from the reader's architecture. Usage `max` describes this kernel sentinel, rather than an unlimited configured limit. | [DMEM output routines](https://github.com/torvalds/linux/blob/v7.3-rc1/kernel/cgroup/dmem.c) |
| CPU bandwidth counters | Kernel signed 32-bit counters can print negative values after overflow. Decode their unsigned 32-bit bit pattern into `Count`; document wraparound after 2^32 events. | [CPU counter storage](https://github.com/torvalds/linux/blob/v6.13/kernel/sched/sched.h), [CPU output](https://github.com/torvalds/linux/blob/v6.13/kernel/sched/core.c) |
| `irq.pressure` | IRQ PSI has only a `full` line, so it needs a separate snapshot/parser. Added read-only Rust and Python readers. | [Linux 6.1 PSI](https://github.com/torvalds/linux/blob/v6.1/kernel/sched/psi.c) |
| `cgroup.procs`, `cgroup.threads` | Preserve order, duplicates, empty lists, and zero for tasks invisible in the reader's PID namespace. Do not confuse observed list lengths with hierarchical task counts. | [core membership output](https://github.com/torvalds/linux/blob/v6.18/kernel/cgroup/cgroup.c), [PID namespace conversion](https://github.com/torvalds/linux/blob/v6.18/include/linux/pid.h) |
| `io.stat` | Preserve optional latency and cost fields. `win` uses milliseconds, `avg_lat` and cost times use microseconds, and `delay_nsec` uses nanoseconds. `depth` accepts `max`; `use_delay=-1` is a fixed-delay indicator. Window counters can reset. | [latency output](https://github.com/torvalds/linux/blob/v6.18/block/blk-iolatency.c), [cost output](https://github.com/torvalds/linux/blob/v6.18/block/blk-iocost.c), [delay output](https://github.com/torvalds/linux/blob/v6.18/block/blk-cgroup.c) |
| `io.cost.qos` | Scaling percentages can exceed 100%, unlike PSI averages. Keep typed ratios without imposing PSI's upper bound. | [I/O cost parameters](https://github.com/torvalds/linux/blob/v6.18/block/blk-iocost.c) |
| `misc.events` | Failures include hardware-capacity boundaries as well as configured limits. Clarified hierarchical and local counter scope. | [misc charging and events](https://github.com/torvalds/linux/blob/v6.18/kernel/cgroup/misc.c) |

The memory statistics audit also added the missing `pgpromote_success` page
metric (Linux 6.12), including NUMA parsing. All current upstream `memory_stats`
keys have typed byte, page, or event-count representations. Unknown future keys
remain ignored because their units cannot be inferred safely.

## Added readable interfaces and availability

| Interfaces or fields | Mainline introduction |
| --- | --- |
| `pids.peak`, `cgroup.pressure`, `irq.pressure` | Linux 6.1 |
| `pids.events.local` | Linux 6.11 |
| `cgroup.stat.local` (`frozen_usec`) | Linux 6.18 |
| `cgroup.procs`, `cgroup.threads`, `cgroup.freeze`, `cgroup.max.depth`, `cgroup.max.descendants` | Available before the 5.15 baseline |
| `cpuset.cpus.exclusive`, `cpuset.cpus.exclusive.effective` | Linux 6.7 |
| `cpuset.cpus.isolated` | Linux 6.8 |
| `io.cost.qos`, `io.cost.model`, `io.prio.class` | Available before the 5.15 baseline |
| `cpu.stat` `core_sched.force_idle_usec` | Linux 6.0, with core scheduling |
| `cpu.stat` `nice_usec` | Linux 6.13 |
| `dmem.peak` | Linux 7.3 release candidates; not present in 7.2 |

Public interface rustdoc links to the corresponding versioned sources.
Configuration, controller enablement, root exemptions, runtime accounting,
and distribution backports also affect availability. Local freezer accounting
measures requested freezing time, rather than only completed frozen time.
The PSI accounting switch is distinct from actual pressure measurements.

## Documentation reconciliation

Corrected memory-stat introduction notes: `pswpin`/`pswpout` are from 6.13,
`zswpin`/`zswpout` from 5.19, and `zswpwb` from 6.8. Added notes for newer
reclaim and THP swap counters. Zswap writeback's ancestor policy applies since
6.11; the configured local boolean alone does not describe effective policy.
IRQ PSI exists since 6.1; runtime-disabled IRQ accounting reports an unsupported
operation since 6.14. Root and hierarchy behavior are documented per interface.
Swap events include descendants unless the cgroup2 mount uses `memory_localevents`,
which stops propagation to ancestors for both memory and swap events; see the
[event propagation implementation](https://github.com/torvalds/linux/blob/v6.12/include/linux/memcontrol.h).

Reconciled the Rust/Python READMEs, stubs, TODO inventory, project direction,
release checklists, and convenience-design reader inventory. Historical release
artifact checks remain historical and require repetition for the expanded API.

## Scope and verification

This audit covers generic cgroup v2 readable interfaces and documented
statistics, including the HugeTLB reservation interfaces and newer RDMA/device
memory output. BFQ-specific scheduler tuning/debug interfaces, arbitrary BPF/perf
introspection, and write-only actions (`cgroup.kill`, `memory.reclaim`) are
outside this read-only surface. Notifications retain their explicitly documented
supported-file subset; reading PSI never installs writable threshold triggers.

Regression fixtures cover valid, malformed, optional, and older-kernel output,
checked time conversions, sentinel values, and immutable Python mappings.
Hardware-dependent controllers are checked against source and fixtures;
their absence in a live environment is not a measured zero. Snapshot getters
inspect stored values, while readers perform fresh I/O; separate reads remain
non-atomic.

Validation passed: the full devenv Python-profile checks; Linux workspace
tests (96 crate unit tests, 8 live-test entry points, 28 native binding tests,
and 10 tooling tests); Linux strict Clippy, doctests, and rustdoc; and 36
installed-wheel contract tests from outside the checkout. The four Kubernetes
Pod-specific checks were skipped in that contract run. The isolated Linux 5.15
and 6.18 VM suites each passed all eight live tests, including disposable
cgroup configuration and notification fixtures.
The crate's Rust 1.88 Linux all-target/all-feature check and portable doctests
also passed with the compiler and rustdoc selected explicitly.

Callers must adapt to the corrected sentinel-preserving APIs: Rust
`MemoryMin::value()` and `MemoryLow::value()` now return `MaxOr<Bytes>`, and
`DmemCurrent::regions()` maps to `MaxOr<Bytes>`. Their Python equivalents return
`MaxOr[int]`; check `is_max` before accessing `value`.
