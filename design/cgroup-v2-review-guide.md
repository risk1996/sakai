# Reviewing the cgroup v2 code

Start with [`cpu/max.rs`](../sakai/src/cgroup/v2/cpu/max.rs). It fits the read model, a full write, a quota-only write, and their tests in one file.

1. Find `CpuMax` in [`sakai/src/lib.rs`](../sakai/src/lib.rs)'s two examples. One reads the live file through `Cgroup`; the other parses saved text. Both produce the same owned snapshot.
2. In [`cpu/max.rs`](../sakai/src/cgroup/v2/cpu/max.rs), read the struct, the `positional` codec declaration, and the `ReadWrite` file declaration together. Follow `"25000 100000"` from the two microsecond tokens to `quota` and `period`, then to `cpu_count()`.
3. In [`handle.rs`](../sakai/src/cgroup/v2/handle.rs), find `read_value` and `write_value`, then follow them to `read_file` and `write_file`. The read resolves the filename, reads through the open directory descriptor, and decodes the text. The write resolves the filename, encodes one command, then sends it to the kernel.
4. Compare `write_value::<CpuMax>(&limit)` with `write_value::<CpuQuota>(quota)`. Both target `cpu.max`. The first sends quota and period; the second sends quota alone and lets the kernel retain the existing period.
5. Search for both types in [`linux_live.rs`](../sakai/tests/linux_live.rs), starting at `parses_live_delegated_controller_interfaces`. Check the typed readback, raw kernel text, and the two failure cases. A fractional microsecond fails encoding. An exactly representable period outside the kernel's accepted range reaches the kernel and returns an OS error.

## Public API, discovery, and descriptor I/O

Read these in order to see how a Rust caller gets from a public interface type to an open kernel file.

| File | What it establishes and where to look |
| --- | --- |
| [`sakai/src/lib.rs`](../sakai/src/lib.rs) | Crate exports and the snapshot contract. Start with units, missing counters, and the separation between offline parsing and Linux I/O. |
| [`cgroup/mod.rs`](../sakai/src/cgroup/mod.rs) | Hierarchy entry point. It exposes the unified v2 implementation. |
| [`v2/mod.rs`](../sakai/src/cgroup/v2/mod.rs) | Public controller modules, Linux-only exports, and shared read/write error semantics. |
| [`v2/interface.rs`](../sakai/src/cgroup/v2/interface.rs) | Sealed file declarations. Read `CgroupFile`, `ReadableFile`, and `WritableFile` together to see where name, filename key, decoded value, and command type come from. Then match `cgroup_file!` arms to their uses in `CpuMax` and `CpuQuota`. |
| [`v2/path.rs`](../sakai/src/cgroup/v2/path.rs) | Discovery from unified membership and caller-visible mounts. Start at `CgroupPath::candidates`. Check mount-root prefix matching, candidate ordering, and the deleted-membership suffix in the procfs fixtures. |
| [`v2/handle.rs`](../sakai/src/cgroup/v2/handle.rs) | Directory ownership and the generic operations. Follow `from_pid` through candidate fallback, then `from_path` through directory opening and filesystem verification. Read `child` and `children` for name validation and removal during enumeration. The stored path is for diagnostics. |
| [`v2/file.rs`](../sakai/src/cgroup/v2/file.rs) | Descriptor-relative interface I/O. Look at the single-filename check, open flags, and write loop. An interrupted write is retried. A short write produces an error because sending the remainder would issue another kernel command. |
| [`sakai/src/error.rs`](../sakai/src/error.rs) | Discovery, OS, parse, and encode errors. Follow `interface_operation` for errno classification and `ParseError` for the raw input, field, and underlying cause. Check where the handle adds a diagnostic path. |

## Units and text codecs

Read the specific macro arm used by a controller as you encounter it. `cgroup_value!` describes the text format of a value. `cgroup_file!` declares the operations that Sakai exposes on a kernel file.

| File | What it establishes and where to look |
| --- | --- |
| [`sakai/src/unit.rs`](../sakai/src/unit.rs) | Quantity types and storage units. Check `Time`, `NonZeroTime`, `Count`, `Pages`, and `Ratio` before comparing the same field across interfaces. |
| [`sakai/src/limit.rs`](../sakai/src/limit.rs) | `MaxOr<T>` and its finite-value helpers. Each controller documents the meaning of `max` for its own file. |
| [`parse/mod.rs`](../sakai/src/parse/mod.rs) | `Parser` for positional tokens, `KeyedFields` for named values, and `ParseCgroup<Unit>` for conversions. Trace `ParseNonZeroMicroseconds` and the `MaxOr` codec from the first pass. Check when optional fields return `None` and when malformed input produces an error. |
| [`parse/codec.rs`](../sakai/src/parse/codec.rs) | `EncodeCgroup` and shared `cgroup_value!` implementations. Compare the positional arm with keyed fields, optional groups, and device rows. A snapshot's `encode_cgroup` describes its text representation. The file's `WritableFile::encode` determines what goes to the kernel. |
| [`scalar.rs`](../sakai/src/scalar.rs) | `Scalar<T, I>` and the marker declarations for single-value interfaces. Use `MemoryMax` and `MemoryCurrent` to trace how an alias gets its filename, mode, token codec, and available operations. |
| [`parse/tests.rs`](../sakai/src/parse/tests.rs) | Shared table-test helpers for expected values and missing, invalid, or excess fields. Read alongside an interface's input cases. |
| [`scalar/tests.rs`](../sakai/src/scalar/tests.rs) | Tests for the shared scalar contract. Use them to review parse and encode behavior inherited by the controller aliases. |

## Hierarchy, task state, and notifications

| File | What it establishes and where to look |
| --- | --- |
| [`core.rs`](../sakai/src/cgroup/v2/core.rs) | Core flags, hierarchy limits, topology, controller availability, and task lists. Compare observed `cgroup.type` states with the threaded transition, `cgroup.controllers` with `cgroup.subtree_control`, and observed membership with `MigrationTarget`. |
| [`core/state.rs`](../sakai/src/cgroup/v2/core/state.rs) | `cgroup.events`, `cgroup.stat`, and `cgroup.stat.local`. Compare populated state, optional freeze completion, descendant and subsystem counts, and local frozen time. |
| [`core/task.rs`](../sakai/src/cgroup/v2/core/task.rs) | Observed PID/TID tokens. Check empty lists, ordering, duplicate IDs, and zero for a task invisible in the reader's PID namespace. Then revisit the meaning of zero when encoded as a migration command in `core.rs`. |
| [`pids.rs`](../sakai/src/cgroup/v2/pids.rs) | Current and peak task counts, the limit, and hierarchical/local limit events. Check the required `max` counter, optional file availability, and the effect of `pids_localevents` on the counts. |
| [`notification.rs`](../sakai/src/cgroup/v2/notification.rs) | Supported subscription selectors, `EventPoller`, and `ConfigurationWatcher`. Follow the initial read and acknowledgment on the retained event descriptor. Then follow the inotify watch on the opened configuration inode and check timeout, coalescing, removal, and overflow behavior. |
| [`sakai/src/pressure.rs`](../sakai/src/pressure.rs) | Shared CPU, memory, and I/O PSI snapshots, plus full-only IRQ pressure. Check window averages and total stall time. CPU, memory, and I/O select the shared parser through their own file declarations. Compare these fresh reads with the retained descriptors in `notification.rs`. |

## CPU bandwidth, scheduling, and placement

| File | What it establishes and where to look |
| --- | --- |
| [`cpu/mod.rs`](../sakai/src/cgroup/v2/cpu/mod.rs) | CPU public exports and the `CpuPressure` file declaration. |
| [`cpu/max.rs`](../sakai/src/cgroup/v2/cpu/max.rs) | Quota and period, derived CPU equivalents, and the full and quota-only command forms from the first pass. |
| [`cpu/max_burst.rs`](../sakai/src/cgroup/v2/cpu/max_burst.rs) | The separate burst allowance. Check its wire unit against the shared time codec. |
| [`cpu/stat.rs`](../sakai/src/cgroup/v2/cpu/stat.rs) | CPU time, local throttling, and optional bandwidth and burst accounting. Read the grouped-field declarations and the tests for wrapped signed kernel counters. |
| [`cpu/weight.rs`](../sakai/src/cgroup/v2/cpu/weight.rs) | Typed CPU shares and the idle state reported by `cpu.weight`. Check the range of `Weight` and the write command's accepted type. |
| [`cpu/idle.rs`](../sakai/src/cgroup/v2/cpu/idle.rs) | The separate boolean write path for idle scheduling. Read with `cpu/weight.rs`. |
| [`cpu/weight_nice.rs`](../sakai/src/cgroup/v2/cpu/weight_nice.rs) | The signed nice-value scale and validation for `cpu.weight.nice`. |
| [`cpu/uclamp.rs`](../sakai/src/cgroup/v2/cpu/uclamp.rs) | Minimum and maximum utilization clamps. Check the ratio conversion and the upper file's `max` token. |
| [`cpuset/mod.rs`](../sakai/src/cgroup/v2/cpuset/mod.rs) | Placement exports and the distinction between a configured request and an effective grant. |
| [`cpuset/list.rs`](../sakai/src/cgroup/v2/cpuset/list.rs) | CPU and NUMA node ranges, effective aliases, and the exclusive and isolated CPU lists. Review range validation and how a write clears a configured list. |
| [`cpuset/partition.rs`](../sakai/src/cgroup/v2/cpuset/partition.rs) | Partition state and optional kernel diagnostic text. Compare snapshot encoding with the write command, which emits the base state. |

## Memory, swap, and compression

| File | What it establishes and where to look |
| --- | --- |
| [`memory.rs`](../sakai/src/cgroup/v2/memory.rs) | Memory usage, peak, protections and limits, OOM-group policy, and PSI. Compare `min`/`low` with `high`/`max` and follow the scalar types into `scalar.rs`. |
| [`memory/stat.rs`](../sakai/src/cgroup/v2/memory/stat.rs) | The field catalog for byte amounts, page quantities, and event counts. Look at baseline-field validation and the handling of keys from newer kernels. |
| [`memory/numa_stat.rs`](../sakai/src/cgroup/v2/memory/numa_stat.rs) | Per-node accounting using the field types from `memory/stat.rs`. Follow the `N<id>=<value>` parser and check how missing nodes are represented. |
| [`memory/events.rs`](../sakai/src/cgroup/v2/memory/events.rs) | Hierarchical and local event files with a shared counter shape. Check which counters older kernels may omit. |
| [`memory/swap.rs`](../sakai/src/cgroup/v2/memory/swap.rs) | Swap current/peak usage, high/max bounds, and events. Review the byte units and counter meanings. |
| [`memory/zswap.rs`](../sakai/src/cgroup/v2/memory/zswap.rs) | Compressed swap usage, its cap, and the writeback policy. Check each alias's mode and kernel availability. |

## Block I/O

| File | What it establishes and where to look |
| --- | --- |
| [`io/mod.rs`](../sakai/src/cgroup/v2/io/mod.rs) | I/O exports, PSI declaration, and the shared `Device` identifier and row parser. Start here before the per-device formats. |
| [`io/stat.rs`](../sakai/src/cgroup/v2/io/stat.rs) | Per-device byte and operation counts, discard activity, and optional latency/cost data. Check wire units for debug fields and the special delay representation. |
| [`io/max.rs`](../sakai/src/cgroup/v2/io/max.rs) | Bandwidth and IOPS limits, with complete and partial commands for one device. Check required snapshot fields, omitted patch fields, `max` to clear a limit, and rejection of an empty patch. |
| [`io/weight.rs`](../sakai/src/cgroup/v2/io/weight.rs) | Default weighting, per-device overrides, and the commands to set or clear them. Compare the map returned by a read with the scope of one command. |
| [`io/latency.rs`](../sakai/src/cgroup/v2/io/latency.rs) | Per-device target latency. Check the command for setting or clearing one target and compare its units with the latency statistics. |
| [`io/cost.rs`](../sakai/src/cgroup/v2/io/cost.rs) | Root-level QoS and cost-model settings. Compare the two row types, named control/model values, and the percentage codec with the bounded PSI codec. |
| [`io/priority.rs`](../sakai/src/cgroup/v2/io/priority.rs) | The enum and text tokens for I/O priority classification. |

## Other resource controllers

| File | What it establishes and where to look |
| --- | --- |
| [`hugetlb.rs`](../sakai/src/cgroup/v2/hugetlb.rs) | Page-size-keyed filenames, usage and reservation limits, events, and NUMA placement. Trace a `PageSize` through `Cgroup::read_file` to a filename such as `hugetlb.2MB.current`. |
| [`rdma.rs`](../sakai/src/cgroup/v2/rdma.rs) | Per-device usage, peak, limits, and events. Check how optional indices distinguish same-named devices and how the limit command selects one device. |
| [`dmem.rs`](../sakai/src/cgroup/v2/dmem.rs) | Per-region device-memory usage, capacity, peak, protections, and limits. Check the `max` sentinel in accounting and how a command validates a region name. |
| [`misc.rs`](../sakai/src/cgroup/v2/misc.rs) | Named-resource counts, limits, and events. Review the event suffix parser and the single-resource maximum command. |

## Tests and fixtures

Read each interface's `#[cfg(test)]` module next to its parser or command encoder. Check happy-path values, omitted fields, malformed content, and precise commands. Then use these files to see which behavior is exercised with I/O.

| File or fixture group | What it establishes |
| --- | --- |
| [Procfs fixtures](../sakai/src/cgroup/v2/fixtures/) | `unified`, `bind_mount`, `namespace_root`, `multiple_mounts`, and `deleted` supply `1/cgroup` and `1/mountinfo` to the tests in `path.rs`. Compare both files for each case. |
| [Interface fixtures](../sakai/src/cgroup/v2/fixtures/) | Follow fixture names from their test call sites. `p2*` covers core state and pids, `p3*` covers block I/O, and `p4*` covers cpuset. `p6_core_complete` covers more core/pids interfaces, `p6` and the RDMA variants cover device forms, and the other `p6_*` groups cover HugeTLB, dmem, and misc. Compare older-kernel and root variants. `invalid_type` exercises a decode error with its path and source. |
| [`handle.rs` tests](../sakai/src/cgroup/v2/handle.rs) | The fixture helpers open ordinary directories for controlled reads and missing-file checks. Also look at candidate fallback, preserved parse sources, and the directory-opening tests. |
| [`file.rs` tests](../sakai/src/cgroup/v2/file.rs) and [`notification.rs` tests](../sakai/src/cgroup/v2/notification.rs) | Focused tests of descriptor I/O, command delivery, poll readiness, and watch lifetime. Compare the ordinary-file watch tests with the live kernfs assertions. |
| [`sakai/tests/linux_live.rs`](../sakai/tests/linux_live.rs) | Live current-process reads, root exemptions, and delegated controller tests under `SAKAI_VMTEST`. Read `CgroupFixture` for what the guest sets up, then distinguish checks that configure through raw file writes from those that exercise Sakai's typed writes. |
| [`tools/vmtest/README.md`](../tools/vmtest/README.md) | Commands and platform requirements for running the live suite in a guest with a writable hierarchy. |
| [`tools/vmtest/Containerfile`](../tools/vmtest/Containerfile) | The Linux runner image used when launching the guest from macOS. |
| [`xtask/src/vmtest/mod.rs`](../xtask/src/vmtest/mod.rs) | Local and container VM runner, including guest setup and invocation of the test binary. |
| [`xtask/src/vmtest/kernel.rs`](../xtask/src/vmtest/kernel.rs) | Kernel fixture definitions, smoke/full selection, and fixture staging. |
| [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) | How CI builds the live executable and runs it across the kernel matrix. |

## Python binding pass

After reading the Rust API, start at [`sakai-python/src/linux/handle.rs`](../sakai-python/src/linux/handle.rs) and trace `Cgroup.current().cpu().max()`. The Python API exposes read-only controller readers and notification subscriptions.

| File | What it establishes and where to look |
| --- | --- |
| [`sakai-python/src/lib.rs`](../sakai-python/src/lib.rs) | Linux module entry point and import behavior on other operating systems. |
| [`sakai-python/src/linux.rs`](../sakai-python/src/linux.rs) | PyO3 registration for the public classes and exceptions. |
| [`linux/handle.rs`](../sakai-python/src/linux/handle.rs) | Handle sharing across controller readers, path conversion, hierarchy navigation, and monitor creation. |
| [`linux/errors.rs`](../sakai-python/src/linux/errors.rs) | Rust-to-Python exception and errno mapping, plus the helper that detaches during blocking reads. |
| [`linux/mapping.rs`](../sakai-python/src/linux/mapping.rs), [`linux/limit.rs`](../sakai-python/src/linux/limit.rs), and [`quantity.rs`](../sakai-python/src/quantity.rs) | Shared mapping conversion, Python representation of `max` or a value, and native quantity wrappers. Check these when following a snapshot field into Python. |
| [`linux/core.rs`](../sakai-python/src/linux/core.rs), [`linux/pids.rs`](../sakai-python/src/linux/pids.rs), [`linux/notification.rs`](../sakai-python/src/linux/notification.rs), and [`linux/pressure.rs`](../sakai-python/src/linux/pressure.rs) | Core and task readers, notification timeout and descriptor APIs, and PSI conversion. |
| [`linux/cpu.rs`](../sakai-python/src/linux/cpu.rs), [`linux/cpuset.rs`](../sakai-python/src/linux/cpuset.rs), and [`linux/memory.rs`](../sakai-python/src/linux/memory.rs) | CPU snapshots, placement lists and partitions, and memory/swap/zswap readers. |
| [`linux/io.rs`](../sakai-python/src/linux/io.rs), [`linux/hugetlb.rs`](../sakai-python/src/linux/hugetlb.rs), [`linux/rdma.rs`](../sakai-python/src/linux/rdma.rs), [`linux/dmem.rs`](../sakai-python/src/linux/dmem.rs), and [`linux/misc.rs`](../sakai-python/src/linux/misc.rs) | Wrappers for device, page-size, region, and resource keyed snapshots. Check how the Rust keys and quantity types appear to consumers. |
| [`linux/stubs.rs`](../sakai-python/src/linux/stubs.rs), [`bin/stub_gen.rs`](../sakai-python/src/bin/stub_gen.rs), [`_sakai.pyi`](../sakai-python/python/sakai/_sakai.pyi), and [`__init__.py`](../sakai-python/python/sakai/__init__.py) | Stub generation adjustments, generator entry point, packaged type signatures, and public Python exports. |
| [`linux/tests.rs`](../sakai-python/src/linux/tests.rs), [`test_contract.py`](../sakai-python/tests/test_contract.py), and [`test_quantity.py`](../sakai-python/tests/test_quantity.py) | Shared native test helpers, installed-package API and docstring checks, and quantity behavior. |
| [`test_kubernetes.py`](../sakai-python/tests/test_kubernetes.py) and [Kubernetes test README](../tools/kubernetes/README.md) | An end-to-end check of what an installed wheel reports for a Pod's configured CPU and memory resources. |

## Design background

- [Shared cgroup text codecs ADR](../adr/shared-cgroup-text-codecs.md) explains why the parser and encoder share layout declarations.
- [Linux implementation audit](cgroup-v2-linux-audit.md) records kernel-version and interface edge cases. Compare its references with the version notes in the relevant Rust type.
- [Interface inconsistencies](cgroup-v2-interface-inconsistencies.md) records the review of value, parser, and file conventions preceding the typed interface cleanup.
- [Cgroup v2 convenience functions](cgroup-v2-convenience-functions.md) discusses derived values and their limits, starting with `CpuMax::cpu_count()`.
- [Bindings design](bindings.md) records the API boundary between Rust and Python.
- [Live Linux testing ADR](../adr/e2e_live_linux_test.md) explains the guest-based test setup and its tradeoffs.
