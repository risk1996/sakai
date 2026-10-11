# User-facing conveniences across cgroup v2

Research date: 2026-10-11. Status: design report; no API changes are implemented
by this document. Proposed names and signatures are illustrative, not a release
commitment. Repository observations describe the base revision of this change.

## Recommendation

Sakai should help callers answer resource questions, in addition to exposing
typed kernel files. Start with small, pure conveniences like
`CpuMax::cpu_count()`: placement-list cardinality, device-weight fallback,
usage-versus-limit comparisons, checked counter differences, and fractional
rates. Then build explicit collections of snapshots and bounded ancestor
inspection. Cover the remaining controllers with the same conventions rather
than developing a separate interpretation model for each one.

Keep three distinctions visible throughout the API:

- A configured value, an observed value, and a derived value have different
  meanings.
- This cgroup, its descendants, an ancestor's shared budget, and an individual
  thread are different scopes.
- A calculation can be exact for its inputs while its inputs are incomplete,
  volatile, or insufficient to predict whether an allocation will succeed.

The largest immediate benefits are accurate CPU rates, memory and task-limit
comparisons, per-device I/O rates, and PSI interval calculations. The largest
later benefit is explaining which visible constraint produced a bound, including
when hidden ancestors prevent a complete answer. Automatic worker counts,
cache sizes, and health verdicts belong in explicitly selected policies or
examples, not implicit conversions of metrics.

## Starting point: what `CpuMax::cpu_count()` gets right

[`CpuMax`](../sakai/src/cgroup/v2/cpu/max.rs) stores quota and period as typed,
nonzero times. `cpu_count(&self) -> MaxOr<Ratio>` computes `quota / period`,
including fractional CPU equivalents, and preserves unlimited quota. For
example, `25000 100000` gives `0.25`, while `max 100000` stays `Max`.

Its documentation already explains that this is the local bandwidth ratio,
excluding ancestor limits, affinity, and scheduling policy. This is the pattern
to extend: a useful question, explicit inputs, typed output, no hidden reads,
and an honest statement of scope. Retain this method; an additive
`bandwidth_cpus()` alias could improve discoverability, but a rename is not
needed to start this work. Avoid a second, unrelated method named `cpu_count`
that returns an integer selected by a worker-pool policy.

The report builds on the existing [read-only project direction](project-direction.md),
[interface roadmap](../TODO.md), [coding conventions](../CODE_STYLE.md), and
[resource-introspection research](bindings.md). The latter already distinguishes
CPU quota, CPU placement, affinity, and host resources.

## Scope and evidence

The inventory covers the documented cgroup v2 core and controller interfaces,
source-defined additions relevant to those families, event delivery, and
controllers without ordinary metric files. Write-only actions are identified
so that “all metrics” does not accidentally become a mutation feature.
It does not treat arbitrary tracing, BPF application counters, vendor extensions,
or every host statistic as standard cgroup metrics.

The upstream reference is the [Linux cgroup v2 guide](https://docs.kernel.org/admin-guide/cgroup-v2.html).
It distinguishes weights, limits, protections, and allocations; those concepts
should remain distinct in Sakai. Availability depends on kernel configuration,
controller enablement, root exemptions, namespaces, and kernel version.
This report also checks controller source tables, linked beside the inventory.
Current upstream documentation and `master` sources are moving references;
their presence does not establish availability in a particular released kernel.
Before implementing any new interface, verify its introduction against release
tags and document availability relative to Sakai's Linux 5.15 test baseline.

Recommendations and formulas below are Sakai design proposals. They are not
claims that the kernel provides an atomic resource-summary API.

## Complete interface coverage and opportunities

“Present” means Sakai has a typed read/parser surface in the inspected tree;
it does not mean the proposed convenience exists. “Gap” means additional reader
or parser work is needed. A field inside an existing file can still be a gap.

### Core, lifecycle, and pressure

The core inventory was checked against the
[upstream core interface tables](https://github.com/torvalds/linux/blob/master/kernel/cgroup/cgroup.c).
The table includes PSI interfaces registered by core code rather than assuming
that every pressure file requires its corresponding resource controller.

| Interfaces | Sakai coverage | Proposed user-facing convenience |
| --- | --- | --- |
| `cgroup.events` | Present: populated and optional frozen | `is_empty()` as the inverse of populated; transitions between snapshots; explicit unknown frozen state |
| `cgroup.stat` | Present: descendant and subsystem maps | Per-controller lookup; checked map comparisons; descendant-count changes without calling them creation rates |
| `cgroup.type` | Used internally for handle validation; no public reader | Typed domain/threaded state and predicates that explain the valid scope of an operation |
| `cgroup.procs`, `cgroup.threads` | Gap | Typed observed IDs, explicit deduplication, membership inspection; do not substitute their length for hierarchical task accounting |
| `cgroup.controllers`, `cgroup.subtree_control` | Gap | Supported-controller sets, enabled-for-children sets, and set comparisons |
| `cgroup.max.descendants`, `cgroup.max.depth` | Gap | Usage-versus-bound comparisons for subtree planning, with an explicit relative depth |
| `cgroup.freeze`, `cgroup.stat.local` | Gap | Requested-versus-completed freeze summary; interval fraction from `frozen_usec` with its actual accounting meaning |
| `cgroup.pressure` | Gap | Report PSI accounting configuration as part of pressure availability |
| `cpu.pressure`, `memory.pressure`, `io.pressure` | Present | Shared window selection and checked interval stall ratios |
| `irq.pressure` | Gap | A resource-specific pressure parser and interval report; do not require the `some`/`full` shape of other PSI resources |
| `cgroup.kill` | Write-only action | No metric getter; retain outside this read-only design |

For core helpers, “empty” refers to observed population, not proof that the
directory can be deleted. Membership lists can change during reads and contain
duplicate IDs. Freeze configuration and completion are different observations;
an ancestor can freeze a descendant. `cgroup.stat.local` accounts the requested
freezing interval rather than only time after all tasks complete freezing.
Those behaviors are represented in the
[core implementation](https://github.com/torvalds/linux/blob/master/kernel/cgroup/cgroup.c).
Do not manufacture a `ready_for_deletion()` or `safe_to_migrate()` verdict from
these snapshots.

### CPU

All currently planned CPU readers are present. The proposals extend their
existing types in [`cpu/`](../sakai/src/cgroup/v2/cpu/), keeping usage counters
and bandwidth counters separate.

| Interfaces | Proposed convenience | Important qualification |
| --- | --- | --- |
| `cpu.max` | Existing CPU equivalents; explicit conversion to millicores through `Ratio` units or binding examples | Fractional bandwidth, not an integer worker count |
| `cpu.max.burst` | Burst-to-quota ratio and burst-to-period ratio, given a `CpuMax` snapshot | Extra burst allowance must not be added to sustained CPU equivalents |
| `cpu.stat` usage/user/system | Checked differences; usage in CPU equivalents; user/system fractions of measured usage | Distinguish average CPUs used from percentage of one CPU or of an explicit bound |
| `cpu.stat` periods/throttles/bursts | Throttled-period fraction, burst-period fraction, event rates, mean accounted throttle time per event | Zero denominator is undefined; these are local bandwidth-accounting signals |
| `cpu.stat.local` | Checked runqueue-throttle difference and accounted throttle time per wall second | An aggregate can exceed one wall second per second; do not call it a bounded stall percentage |
| `cpu.weight`, `cpu.weight.nice`, `cpu.idle` | Clear scheduling-state summary; caller-supplied weight comparison | Nice is an approximation; weights are relative priorities rather than reserved CPU counts |
| `cpu.uclamp.min`, `cpu.uclamp.max` | Requested clamp range, preserving an unbounded maximum | Clamps describe utilization requests rather than bandwidth quotas |

Hierarchy and burst behavior warrant particular care: a child's bandwidth can
be stopped by an ancestor, and a short sampling window can reflect burst
behavior. Minimum visible quota/period ratios are useful long-term bounds but
do not predict short-window scheduling. See the
[CFS bandwidth documentation](https://docs.kernel.org/scheduler/sched-bwc.html).
For relative-weight examples, require the caller to supply the compared groups
and label the result a configured comparison. Sakai should not imply that it
has discovered the currently runnable competition.

### Memory, swap, zswap, and NUMA

The inspected tree already exposes all twenty memory interfaces in
[`TODO.md`](../TODO.md), with typed byte/page/event maps in
[`memory.stat`](../sakai/src/cgroup/v2/memory/stat.rs) and
[`memory.numa_stat`](../sakai/src/cgroup/v2/memory/numa_stat.rs).
The underlying interface table and protection calculations are in
[memcontrol.c](https://github.com/torvalds/linux/blob/master/mm/memcontrol.c).

| Interfaces | Proposed convenience |
| --- | --- |
| `memory.current`, `memory.max`, `memory.high` | Local hard-limit and high-threshold comparisons, remaining distance, overage, and finite-limit usage ratio |
| `memory.peak` | Current-to-recorded-peak ratio and peak-to-explicit-limit ratio |
| `memory.min`, `memory.low` | Configured protection comparisons; clearly separate hard and best-effort protection |
| `memory.events`, `memory.events.local` | Checked event differences, per-event rates, `had_oom_kill()` over an interval, local-versus-hierarchical views |
| `memory.stat` | Typed per-field lookup; selected composition ratios; reclaim efficiency, major-fault fraction, and checked event/page rates |
| `memory.numa_stat` | Per-field node totals, node shares, largest reported node, placement comparison by selected field |
| `memory.swap.current`, `memory.swap.max`, `memory.swap.high` | Independent swap-limit and threshold comparisons |
| `memory.swap.peak`, `memory.swap.events` | Recorded-peak comparisons and checked event differences |
| `memory.zswap.current`, `memory.zswap.max` | Independent compressed-store comparisons |
| `memory.zswap.writeback` | Policy summary, retaining the distinction between storage usage and writeback configuration |
| `memory.pressure` | Shared PSI interval and rolling-window helpers |
| `memory.reclaim` | Write-only action; no read convenience |

A memory comparison helps a caller set a cache budget, but its remaining
distance is not allocatable memory. The memory controller accounts overlapping
categories and has distinct protection, reclaim, limit, and OOM behavior.
Effective protection depends on ancestor and sibling conditions rather than
only the smallest configured protection value. Preserve the original fields
and do not sum every `memory.stat` byte field into a fabricated total. See
[memory accounting and protection implementation](https://github.com/torvalds/linux/blob/master/mm/memcontrol.c).

Prefer these explicit derived quantities:

- `slab_reclaimable / slab`, when both fields exist and the denominator is
  nonzero; do not treat overlapping slab counters as additive components.
- `delta(pgsteal) / delta(pgscan)` for a named reclaim path or the matching
  total counters, with missing fields and counter regression preserved.
- `delta(pgmajfault) / delta(pgfault)` as a major-fault fraction, not a cache
  hit rate. Refault counters likewise do not establish a universal hit rate.
- Per-node shares for one byte field such as anonymous memory; do not combine
  byte, page, and event rows or claim that the result measures remote access.
- An explicitly named `usage_minus_inactive_file_estimate`, if requested by
  downstream monitoring users. Mark it as a heuristic and retain the original
  operands; it is not RSS, guaranteed working set, or `memory.available`.

Keep resident memory, swap, zswap storage, and device memory as separate
budgets. A sum of selected memory and swap usage may be offered with its
definition, but it cannot be advertised as all memory charged or all available
memory. Zswap compression ratios require matching same-scope inputs and an
explicit accounting definition; defer a universal compression-ratio method.

Recorded peaks also need provenance. The existing peak-reader docs describe
descriptor-dependent reset behavior. Reads in this design must not reset a
peak, and a decrease cannot automatically be interpreted as a workload's new
historical maximum.

### Task count

[`Pids`](../sakai/src/cgroup/v2/pids.rs) currently reads `current`, `max`, and
`events`. The complete upstream file table also includes `pids.peak` and
`pids.events.local`. Task accounting includes threads, and migration or lowering
a limit can leave usage above that limit. Local and hierarchical event
interpretations also depend on the relevant mount behavior. See
[the PID controller](https://github.com/torvalds/linux/blob/master/kernel/cgroup/pids.c).

| Interfaces | Sakai coverage | Proposed convenience |
| --- | --- | --- |
| `pids.current`, `pids.max` | Present | Remaining task slots, overage, usage ratio, and visible ancestor residual comparisons |
| `pids.events` | Present | Checked limit-event differences and rates |
| `pids.peak`, `pids.events.local` | Gap | Peak-to-limit comparison; local event differences |

Call the derived value `remaining_tasks`, not `remaining_processes` or
`can_fork`. Fork/clone can fail for reasons beyond cgroup PID limits.

### Block I/O

[`io/`](../sakai/src/cgroup/v2/io/) currently supplies `stat`, `pressure`,
`weight`, `max`, and `latency`. In addition to new files, newer or optional
fields in `io.stat` need a separate coverage check; the current stat snapshot
exposes the six read/write/discard byte and operation counters.

| Interfaces or fields | Sakai coverage | Proposed convenience |
| --- | --- | --- |
| `io.stat` byte and operation counters | Present | Checked per-device deltas, fractional B/s and IOPS, mean bytes per operation |
| `io.weight` | Present | `weight_for(device)` returns an override or the stored default |
| `io.max` | Present | Local per-direction bandwidth/IOPS ratios using an interval rate; explicit absent-device fallback |
| `io.latency` | Present | Per-device target lookup and observed-latency comparison when a compatible observation exists |
| `io.stat` latency/debug/cost additions | Gap | Typed optional queue depth, latency/window or missed/total observations, delay and cost fields with their actual counter/gauge meanings |
| `io.cost.qos`, `io.cost.model` | Gap | Root-level per-device configuration summaries and typed parameter lookup |
| `io.prio.class` | Gap | Typed priority-policy summary |
| `io.pressure` | Present | Shared PSI conveniences |

Use [`blk-iocost.c`](https://github.com/torvalds/linux/blob/master/block/blk-iocost.c)
to ground cost-model units and root-only configuration. Model coefficients are
not guaranteed throughput, and a weight-based controller's dynamic cost model
does not reduce to a device-wide “percent busy” from byte counters.
Use [`blk-iolatency.c`](https://github.com/torvalds/linux/blob/master/block/blk-iolatency.c)
for latency fields: rotational and non-rotational devices expose different
observations, and missed/total describe a changing evaluation window. Do not
apply monotonic counter-difference helpers to rolling-window fields.
The policy alternatives for `io.prio.class` are defined by
[`blk-ioprio.c`](https://github.com/torvalds/linux/blob/master/block/blk-ioprio.c);
represent known alternatives with an extensible enum.

For `io.max`, a missing row in a successfully read file can be resolved as no
local limit for that device; a missing file is a different result. Preserve
read/write direction and separate B/s from IOPS. The kernel may permit bursts,
so a measured rate above the configured value is not by itself a parser or
calculation error. See
[block I/O throttling](https://github.com/torvalds/linux/blob/master/block/blk-throttle.c).

Do not expose a generic `io_utilization()` without a denominator. Useful
quantities are `read_bandwidth_limit_ratio`, `write_iops_limit_ratio`, and
selected-device totals. Summing every device may double-count storage-stack
layers. Device labels and filesystem-to-block-device resolution require sysfs,
mount, and topology information outside the cgroup snapshot; make those optional
enrichment rather than hidden I/O in a getter.

### CPU and NUMA placement

[`CpuList` and `MemoryNodeList`](../sakai/src/cgroup/v2/cpuset/list.rs) already
store compact inclusive ranges and expose `ranges()`, lazy `ids()`, and
`contains()`. Extend these before adding an expanded-ID cache.

| Interfaces | Sakai coverage | Proposed convenience |
| --- | --- | --- |
| `cpuset.cpus`, `cpuset.cpus.effective` | Present | Range-based cardinality, emptiness, intersection, difference, subset testing |
| `cpuset.mems`, `cpuset.mems.effective` | Present | The same operations on a distinct node-list type |
| `cpuset.cpus.partition` | Present | Named state predicates retaining invalid reasons |
| `cpuset.cpus.exclusive`, `cpuset.cpus.exclusive.effective` | Gap | Requested-versus-granted exclusive placement comparisons |
| `cpuset.cpus.isolated` | Gap | Root-level isolated CPU inventory and explicit intersections |

The additional placement files and root flags are grounded in
[the v2 cpuset file table](https://github.com/torvalds/linux/blob/master/kernel/cgroup/cpuset.c).
An effective cpuset reports placement, not per-thread affinity or bandwidth.
Configured emptiness and effective emptiness have different meanings, already
documented by Sakai. Do not replace an empty effective list with the host count.

`cardinality()` should return typed `Count` and perform O(number of ranges)
work. Compute each inclusive length in `u64`: `0..=u32::MAX` contains
4,294,967,296 IDs. It must work on a 32-bit target without `usize` overflow or
enumerating that range. Set operations should preserve compact ranges and never
mix CPU IDs with NUMA node IDs. Two adjacent ranges may be normalized in a
derived result without changing the parser's current contract.

### HugeTLB

Sakai has no HugeTLB reader yet. Inspect the
[v2 HugeTLB source template](https://github.com/torvalds/linux/blob/master/mm/hugetlb_cgroup.c)
for the complete per-page-size surface, including reservation interfaces not
listed alongside the main usage files in the guide.

| Interface family | Proposed convenience |
| --- | --- |
| `hugetlb.<size>.current`, `.max` | Usage comparison per typed hugepage size; whole hugepages represented by a byte amount |
| `hugetlb.<size>.rsvd.current`, `.rsvd.max` | Separate reservation-accounting comparison |
| `hugetlb.<size>.events`, `.events.local` | Checked limit-event differences |
| `hugetlb.<size>.numa_stat` | Per-node byte shares for a selected hugepage size |

Discover supported page-size interfaces instead of hard-coding `2MB` and `1GB`.
Keep fault-time usage and reservation accounting distinct; do not add them.
Report the byte remainder when converting a limit into whole hugepages.
Capacity in a host hugepage pool requires additional host interfaces; a cgroup
remaining-distance calculation cannot promise successful allocation. Do not
import v1 `usage_in_bytes`, `failcnt`, or `max_usage_in_bytes` filenames into
the v2 reader.

### RDMA

Sakai has no RDMA reader yet. Current upstream exposes `rdma.current`,
`rdma.max`, `rdma.peak`, `rdma.events`, and `rdma.events.local`, with handle
and object resources. Newer output can disambiguate identical device names
using an index. Keep device identity capable of representing that form and
older name-only forms. See
[the RDMA controller implementation](https://github.com/torvalds/linux/blob/master/kernel/cgroup/rdma.c).

Provide typed resource lookup, current-versus-limit and peak comparisons,
checked resource-specific event differences, and the tightest visible residual
for each device/resource pair. Preserve absent rows and unknown future resource
keys rather than treating every missing device as zero usage. These counts
measure resource accounting, not network throughput or packets. A device
identity can change across samples; detect that before deriving a rate.

### Device memory

Sakai has no device-memory reader yet. Include `dmem.capacity`, `dmem.current`,
`dmem.peak`, `dmem.max`, `dmem.min`, and `dmem.low`, keyed by region identity
and measured in bytes. Capacity is a root-level interface. The source currently
registers `current` without a non-root-only flag, despite the guide describing
it as non-root: record this compatibility discrepancy and probe the requested
interface rather than inventing a uniform root policy. See
[the DMEM file table](https://github.com/torvalds/linux/blob/master/kernel/cgroup/dmem.c).

Provide per-region limit comparisons, peak ratios, visible ancestor residuals,
and capacity-relative usage when root capacity is actually visible. Retain
the region identifier as a validated opaque identifier, not a path to open.
Do not assume every region name identifies a GPU, or combine regions into one
allocation budget. As with memory protection, configured min/low are not a
guaranteed exclusive allocation. This supports accelerator-workload budgeting
without pretending to cover every driver's allocator or all physical VRAM.

### Miscellaneous resources

Sakai has no misc reader yet. The relevant files are `misc.capacity`,
`misc.current`, `misc.peak`, `misc.max`, `misc.events`, and
`misc.events.local`. Capacity is root-only; current and peak can exist at root.
Resource names are extensible, and charges remain with their owner until freed
rather than following every process migration. See
[the misc controller implementation](https://github.com/torvalds/linux/blob/master/kernel/cgroup/misc.c).

Offer resource-keyed counts, usage/limit/capacity comparisons, peak comparisons,
and checked event differences. Keep a resource's identity in every comparison:
different scalar resources cannot be combined into a meaningful total.
Do not hard-code a confidential-computing resource list or call remaining
distance “available encrypted VMs.” Actual allocation can have additional
platform constraints.

### Controllers without ordinary metric files

The device controller uses cgroup BPF rather than a v2 `devices.list` metric,
and perf events can be filtered by cgroup. Those are distinct integration
surfaces, as described by the
[v2 controller guide](https://docs.kernel.org/admin-guide/cgroup-v2.html#device-controller).
Likewise, do not invent cgroup v2 `net_cls`, `net_prio`, network-byte counters,
or block-device capacity files by borrowing v1 names or host metrics.

A later optional integration could identify attached BPF programs or collect
explicitly configured perf/network observations. Label privileges, provider,
and collection lifetime. Arbitrary BPF policy cannot in general be converted
into a static device allowlist. These integrations have a lower priority than
finishing standard read metrics and should not become prerequisites for a
basic cgroup summary.

## Shared contracts for reliable conveniences

### Usage and limit comparison

Use a small shared result for bytes, task counts, hugepage accounting, RDMA,
device memory, and misc resources. It should retain the original typed usage
and `MaxOr<T>` limit; named accessors can derive the following:

```text
remaining(U, max) = max
remaining(U, L)   = max(L - U, 0)
overage(U, max)   = 0
overage(U, L)     = max(U - L, 0)
ratio(U, max)    = undefined: no finite denominator
ratio(U, L > 0)  = U / L
ratio(U, 0)      = undefined: zero denominator
```

Example: usage 120 tasks against a limit of 100 gives remaining 0, overage
20, and ratio 1.2. Usage 0 against limit 0 has no defined ratio. It does not
become 0% or 100%. Unlimited has unlimited local remaining distance, not
infinite host capacity. An absent limit read is not unlimited.

For a proposed `LimitComparison<T>` snapshot, prefer
`remaining(&self) -> MaxOr<T>` and `overage(&self) -> T` on the supported
nonnegative quantities, and a ratio result that distinguishes no finite limit
from zero denominator. Avoid a broadly public arithmetic trait solely to make
this generic: start with existing quantity types and private shared arithmetic
if that keeps the API smaller. A hypothetical `RatioUnavailable` enum should
be extensible. A missing input belongs to collection/availability reporting,
not this complete-input comparison type.

Use different constructors or domain wrappers for `memory.high`, hard limits,
and capacity comparisons. Call distance to `memory.high` a threshold distance.
Never return a single generic “safe resource budget” across those concepts.

### Checked differences and interval rates

Provide differences on cumulative counters, and changes on gauges. They must
be separate operations. Use checked subtraction: when a cumulative counter
decreases, return a field-specific discontinuity rather than wrapping or
saturating it into a plausible rate. A counter that exists in only one sample
is unavailable for that interval, not a measured zero.

Proposed placement: `newer.checked_delta(&older)` on snapshot types, followed
by rate accessors on a domain-specific delta. Keep helper methods pure and
borrow receivers. A higher-level interval object can validate identity and time
ordering before invoking them; pure parsers alone cannot validate cgroup
identity. Required-field failure should fail the required delta, while optional
fields should retain independent availability/regression outcomes so one newer
kernel field does not discard otherwise useful CPU or I/O usage.

For positive elapsed wall time `dt`, useful formulas are:

| Quantity | Formula | Output |
| --- | --- | --- |
| Average CPUs used | `delta(cpu_usage_time) / dt` | `Ratio`; can exceed 1 |
| CPU percentage relative to a named bound `C` | `average_cpus / C` | `Ratio`, undefined when `C = 0` |
| User-time fraction | `delta(user_time) / delta(usage_time)` | `Ratio`, undefined with no measured usage |
| Throttled-period fraction | `delta(nr_throttled) / delta(nr_periods)` | `Ratio`, undefined with no measured periods |
| Event rate | `delta(events) / dt` | Fractional events per second |
| I/O bandwidth | `delta(bytes) / dt` | Fractional bytes per second |
| IOPS | `delta(operations) / dt` | Fractional operations per second |
| Mean transfer size | `delta(bytes) / delta(operations)` | Fractional bytes per operation |
| PSI interval stall ratio | `delta(stall_total) / dt` | `Ratio`, associated with its resource and stall kind |

Example: 1 event over 2 seconds is 0.5 events/s; 1 byte over 2 seconds is
0.5 B/s. The existing `EventRate` uses integer events per nanosecond, and
`BytesPerSecond`/`OperationsPerSecond` use integer storage. Directly dividing
the existing integer quantities can truncate these observations. Introduce
appropriately dimensioned fractional `uom` rate quantities for measured rates,
while keeping exact integer kernel limits and raw counters. Convert before
division, validate positive finite denominators, and document `f64` precision
loss for sufficiently large counters. Subtract integers before converting,
so two large cumulative values do not erase a small exact delta.

A falling memory gauge means usage decreased, not that a counter reset. Gauge
change can use a direction plus unsigned magnitude; avoid casting an arbitrary
`u64` difference into `i64`. Rolling latency-window statistics, descendant
counts, current usage, and peak values are not monotonic event counters.

### Pressure helpers

PSI already supplies rolling averages and cumulative stall totals. An enum such
as `PressureWindow::{Seconds10, Seconds60, Seconds300}` can select an existing
average. A checked total difference gives a caller-selected interval rather
than averaging the rolling averages. `some` and `full` overlap; never add them.
PSI trigger registration requires a write and a persistent descriptor, so it
cannot silently occur in a read-only watcher. See
[the PSI interface and trigger documentation](https://docs.kernel.org/accounting/psi.html).

A convenience such as `average_exceeds(window, caller_threshold)` is useful
only if the threshold and missing-kind behavior are explicit. Do not ship an
arbitrary universal “pressure is high” threshold. Extend the shared parser only
when the resource's line shape is compatible; IRQ pressure requires its own
documented shape.

In the inspected upstream implementation, IRQ pressure reports only a `full`
line. Sakai's current `Pressure` parser requires `some`, so reusing it unchanged
would reject valid IRQ data. Verify the format and support conditions against
the target release before adding the reader; see
[PSI output generation](https://github.com/torvalds/linux/blob/master/kernel/sched/psi.c).

### Map resolution and aggregation

`IoWeight::weight_for(device)` is especially close to the motivating CPU
helper: one snapshot plus a key answers a common question without extra I/O.
Keep `devices()` for callers needing the explicit overrides. An analogous
`IoMax::local_limits_for(device)` can resolve absent rows only after successful
parsing of the file, with a result recording whether a row was explicit.

Add named typed lookups to memory and NUMA maps rather than a generated getter
for every optional field. Missing keys remain missing. For totals, require a
selected field or caller-selected device/resource set, check integer addition
for overflow, and retain completeness information. Empty maps, absent files,
missing fields, new devices, and disappearing devices are separate cases.
Device addition/removal must not be counted as I/O since boot over the latest
sampling interval.

## Collections, hierarchy, and application policy

### Explicit collections of snapshots

A `sample(selection)` reader could collect only requested interfaces, recording
a monotonic collection start/end, a cgroup identity scoped to the sampling
session, and a result for each requested interface. Identity should rely on a
pinned handle or verified session identity rather than a display path that can
be reused. Moving the current process does not make the old handle follow it;
callers must choose whether to keep observing a cgroup or rediscover membership.

Represent independently requested results without turning every failure into
`None`. Keep successful snapshots alongside missing interfaces, permission
errors, and parse errors. Direct readers continue to return the existing errors.
A convenience summary should report what it obtained and the duration of
collection, without claiming atomicity or solving races through timestamps.
Do not eagerly read every controller to calculate a CPU rate.

### Visible ancestor inspection

Add a dedicated bounded reader before advertising hierarchy-aware helpers.
The present handle exposes children and pins a directory, but does not provide
a public ancestor-resolution contract. Traversal must retain the established
cgroup2/mount validation, stop at its declared visible boundary, avoid treating
mount parents as cgroups, and never attempt to escape a namespace for an answer.

Suggested output contains the computed bound, contributing cgroups and values,
sampling span, traversal endpoint, and completeness. Reaching a namespace or
bind-mount boundary does not prove that no stricter ancestor exists. Even a
fully visible hierarchy cannot produce a guaranteed share under competition.
Return a partial observed bound with the reason, or an explicit unavailable
result when inputs are insufficient.

For an ancestor chain that includes this cgroup:

```text
visible_cpu_bandwidth_bound = min(all finite quota / period ratios)
visible_memory_residual    = min(max(limit_i - usage_i, 0))
visible_task_residual      = min(max(limit_i - current_i, 0))
```

The residual expressions include only the relevant finite limits and their
matching hierarchical usage. All successfully observed unlimited limits give
an unbounded cgroup-limit result at that visibility scope. Missing usage for a
finite limit prevents a complete residual result. A missing root-exempt file
can be handled only when the reader established that specific exemption;
generic `FileMissing` must not be relabeled unlimited.

The memory formula is a remaining distance to observed limits, not an
allocation guarantee. Crucially, subtract each ancestor's own usage. Suppose
a child uses 100 MiB with a 1 GiB local limit, while its parent uses 900 MiB
with a 1 GiB limit. Local remaining distance is 924 MiB; the visible shared
parent residual is 124 MiB. Subtracting the child's usage from the parent's
limit would hide sibling consumption. Similar residuals apply to task slots
and matched HugeTLB/RDMA/DMEM/misc resources.

I/O bounds require the same device identity and direction at each ancestor,
but there is no instantaneous cumulative-byte residual for a B/s limiter.
Compare configured rate bounds and measured interval rates instead. Memory
low/min protection and weight shares need controller-specific interpretation;
they are not additional applications of `min(limit_i)`.

### CPU placement, bandwidth, and worker policy

Given explicit placement and per-thread affinity inputs, a proposed summary
can show both allowed logical CPUs and a visible bandwidth bound:

```text
allowed_logical_cpus = cardinality(effective_cpuset intersect thread_affinity)
nominal_cpu_bound    = min(allowed_logical_cpus, visible_bandwidth_bound)
```

Call it a nominal bound with provenance, not guaranteed effective capacity.
If affinity or the controller is unavailable, retain that fact and the
independent values rather than inventing an intersection. Scheduler policy,
sharing, hotplug, and configuration changes can affect actual service. One
thread's mask does not describe every thread in the process.

Keep integer worker decisions separate. For a 2.5-CPU bound, flooring to 2
and rounding up to 3 are different useful policies; for 0.25 CPUs, rounding
to at least 1 expresses an application choice. A zero observed placement grant
must not silently turn into 1. Expose any rounding, minimum, maximum, safety
margin, and refresh policy explicitly. Likewise, a cache-size example can use
a caller-supplied reserve against a named memory comparison, but must not
pretend that Sakai knows the application's allocator or future consumption.

### Notifications

Build a read-only watcher for interfaces with documented change notification,
using notifications as a prompt to reread. Cover core population/freeze,
controller event counters, and partition changes first. Report initial state,
coalescing, read failures, cgroup removal, and cancellation; notifications are
not one-to-one event records. PSI threshold triggers are a separate opt-in
write feature. A portable polling adapter may be useful, but advertise its
interval and overhead rather than implying every file supports `poll`.

## API and binding shape

Place snapshot conveniences on their owning structs or enums, matching
`cpu_count(&self)`. Pure derivation should work on non-Linux platforms with
parsed fixture snapshots. Linux collection, identity, ancestor traversal, and
notifications remain on handles or explicitly named readers. Fallible
derivation errors should identify the operand or field and should not be
misreported as kernel read errors.

Avoid putting memory-specific methods on every `Scalar<T, I>` merely because
several interfaces share `Bytes`. Start with domain comparison types and shared
internal calculations. Preserve raw values, existing readers, `MaxOr`, and
optional fields. Prefer small focused additions over a trait framework or
a single summary that requires every controller to exist.

For Python, follow existing bindings but keep sampling and I/O as methods.
Cheap immutable derived values may be properties. Mirror unlimited, missing,
and invalid arithmetic outcomes instead of using `float('inf')`, zero, or an
undifferentiated `None` for all three. Expose fractional rates with documented
units such as bytes/s and events/s, and preserve counter integers. CPU
equivalents stay floats; task cardinalities stay integers. Bind only completed
Rust behavior and update stubs and representative contract coverage together.

## Suggested delivery order

The following phases are recommendations, not edits to the accepted
[`TODO.md`](../TODO.md) release scope.

| Phase | Work | User benefit | Effort and dependencies |
| --- | --- | --- | --- |
| 1 | List cardinality/emptiness, device-weight resolution, focused typed map lookup, complete-input limit comparisons | Eliminates recurring boilerplate with explicit local semantics | Small; existing snapshots; agree zero/unlimited ratio outcomes |
| 2 | Checked CPU, event, I/O, and PSI differences plus fractional rates | Makes monitoring calculations reliable | Medium; rate units, optional-field outcomes, positive elapsed time |
| 3 | Selected-interface collection and domain summaries | Combines useful reads with timing and partial-error reporting | Medium; session identity and selection design |
| 4 | Bounded ancestor inspection with provenance; explicit CPU and shared-residual summaries | Explains visible constraints for budgeting | Larger; namespace boundaries, same-resource joins, partial results |
| 5 | Missing standard readers: core metadata/state, PID peak/local events, exclusive/isolated cpusets, I/O cost/priority/debug fields, IRQ pressure | Completes common configuration and diagnostic questions | Medium; per-interface availability research and parser contracts |
| 6 | HugeTLB, RDMA, DMEM, misc readers with comparisons and events | Brings specialist workloads the same conveniences | Larger; dynamic keys, controller-specific identity and fixtures |
| 7 | Read-only notifications and explicit application-policy adapters | Reduces repeated sampling and supports caller-selected decisions | Larger; async/lifetime design; preserve read-only PSI scope |

Notification work can follow the existing P5 ordering if release priorities
require it. Specialist controllers should reuse completed comparison/delta
contracts when their readers land. Do not delay small pure helpers until the
entire controller inventory is implemented.

Defer universal health scores, automatic parallelism changes, promised
allocatable memory, guessed bandwidth from weights, whole-system resource
aggregation, implicit host fallbacks, PSI trigger writes, and arbitrary BPF
policy interpretation. These require extra inputs or policies that the
convenience layer cannot infer reliably.

## Validation plan for implementation

This report changes documentation only. Future implementation should use
focused table-driven tests for behavior with real failure modes:

| Area | Essential cases |
| --- | --- |
| Limit comparisons | Unlimited, zero limit, zero usage, exact limit, overage, maximum integer, undefined ratio reasons |
| Placement | Empty list, disjoint/intersecting ranges, adjacent ranges, full `u32` range, CPU/node type separation |
| Differences/rates | Equal counters, regression, 1 event over 2 seconds, large counters with tiny delta, zero/negative ordering, optional fields missing independently |
| Map joins | Empty map, missing key, explicit unlimited row, unknown resource, device addition/removal/reuse, checked aggregation overflow |
| CPU | Fractional quota, different ancestor periods, local unlimited with finite ancestor, aggregate throttle time above elapsed time, burst/configuration changes |
| Memory/PID hierarchy | Parent residual smaller than child residual, missing ancestor input, sibling consumption, over-limit gauge, hidden ancestor, root exemptions |
| PSI/latency | Missing optional line, different resource shapes, overlapping stall kinds, changing window counters, no implicit trigger registration |
| Collection | Partial successful reads, permission and parse errors retained, path reuse, process migration, non-atomic timing, unselected controller not read |
| Specialist controllers | Multiple hugepage sizes, reservation separation, RDMA duplicate names/index forms, opaque regions, unknown misc resources, unavailable root capacity |
| Notifications | Initial state, coalescing, removal, cancellation, reread failure, no controller/configuration writes |

Use OS-independent parser/derivation tests and the existing Linux VM/live suite
for interface and hierarchy behavior. Extend supported/older/root fixtures;
check released kernel versions and distribution backports before claiming
“Since Linux x.y.” Follow the existing Rust/Python documentation and binding
checks, with formatting, Clippy, tests, doctests, and strict rustdoc coverage
where code changes require them. Do not introduce fake monotonic guarantees or
turn unavailable counters into zeros merely to simplify a test.

## Decisions needed before implementation

The inventory and recommended first helpers are concrete enough to implement
incrementally. Settle the following contracts in each relevant change:

1. The public representation of undefined ratios and optional-field
   discontinuities, keeping unavailable inputs separate from complete inputs.
2. Names and dimensions for fractional measured rates, without changing exact
   kernel limits or raw counter storage.
3. Sampling-session identity and how callers choose fixed-cgroup observation
   versus following current-process membership.
4. Ancestor traversal endpoints, provenance, and completeness semantics,
   including bind mounts and namespace roots.
5. Whether heuristic working-set estimates and worker/cache policy adapters
   belong in examples, an opt-in module, or a later package.

The first implementation should deliver placement cardinality, I/O weight
resolution, and complete-input usage comparisons. The next should deliver
checked interval calculations. Together they extend the usefulness of
`CpuMax::cpu_count()` across today's metrics while establishing contracts that
also work for the full cgroup v2 read surface.
