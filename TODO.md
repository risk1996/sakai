# cgroup v2 metrics TODO

The implemented P0–P6 interfaces are in the initial release scope on both
the Rust and Python sides. This inventory covers generic cgroup v2 interfaces
and documented statistics, including optional newer-kernel files. Scheduler-
specific BFQ tuning/debug interfaces and write-only actions are outside this
read surface. P7 and later work is deferred.

An interface item is complete when it has a typed public reader, key-based
parsing where applicable, fixtures for supported and missing/older-kernel
forms, documented volatility and units, and a Linux live test where the file
is expected to exist. Missing optional files must return `FileMissing`.

## P0 — CPU

- [x] `cpu.stat` — usage, user, system, and optional bandwidth counters; parse
      times as microseconds and tolerate bandwidth keys missing when the
      controller is disabled.
- [x] `cpu.max` — quota and period as `MaxOr<NonZeroTime>` and `NonZeroTime`;
      retain the existing typed parser and add the cgroup reader/live coverage.
- [x] `cpu.pressure` — shared PSI `some`/`full` averages and total stall time;
      never write to the file on the read path.
- [x] `cpu.weight` — map `0` to `CpuWeight::Idle` and `1..=10000` to
      `CpuWeight::Shares(Weight)`.
- [x] `cpu.stat.local` — local throttled time, including its older-kernel
      `FileMissing` behavior.
- [x] `cpu.uclamp.max` — `max` or a ratio expressed in hundredths of a percent.
- [x] `cpu.uclamp.min` — ratio expressed in hundredths of a percent.
- [x] `cpu.max.burst` — burst duration in microseconds, not a frequency.
- [x] `cpu.weight.nice` — validated nice value in `-20..=19`.
- [x] `cpu.idle` — boolean idle scheduling state.
- [x] Optional `cpu.stat` nice and forced-idle time counters.

## P1 — Memory

- [x] `memory.current` — current hierarchical memory usage in bytes.
- [x] `memory.max` — hard limit as `MaxOr<Bytes>`.
- [x] `memory.high` — throttling limit as `MaxOr<Bytes>`.
- [x] `memory.events` — hierarchical low/high/max/OOM counters, preserving
      version-dependent optional keys.
- [x] `memory.stat` — split byte, page, and count fields into distinct typed
      groups and ignore unknown keys.
- [x] `memory.pressure` — shared PSI `some`/`full` averages and total stall
      time; never write to the file on the read path.
- [x] `memory.peak` — peak usage in bytes; document that reset semantics are
      per file descriptor on kernels that support writes.
- [x] `memory.low` — best-effort memory protection as `MaxOr<Bytes>`.
- [x] `memory.min` — hard memory protection as `MaxOr<Bytes>`.
- [x] `memory.oom.group` — boolean group OOM policy.
- [x] `memory.events.local` — local, non-hierarchical memory event counters.
- [x] `memory.swap.current` — current swap usage in bytes.
- [x] `memory.swap.max` — hard swap limit as `MaxOr<Bytes>`.
- [x] `memory.swap.events` — swap high/max/fail counters.
- [x] `memory.swap.high` — swap throttling limit as `MaxOr<Bytes>`.
- [x] `memory.swap.peak` — peak swap usage in bytes.
- [x] `memory.numa_stat` — typed byte, page, and event totals per NUMA node.
- [x] `memory.zswap.current` — current zswap memory usage in bytes.
- [x] `memory.zswap.max` — zswap limit as `MaxOr<Bytes>`.
- [x] `memory.zswap.writeback` — boolean zswap writeback policy.

The root cgroup lacks `memory.current` and the memory limit and protection
files. Treat that as `FileMissing`, not a parse failure or panic.

## P2 — Process and cgroup state

- [x] `pids.current` — hierarchical task count, including threads; it may exceed the
      configured maximum.
- [x] `pids.peak` — peak hierarchical task count.
- [x] `pids.max` — process limit as `MaxOr<Count>`.
- [x] `pids.events` — hierarchical process-limit event counters.
- [x] `pids.events.local` — local limit-event counters, accounting for mount options.
- [x] `cgroup.events` — populated and frozen state for lifecycle monitoring.
- [x] `cgroup.stat` — descendant and subsystem state counters.
- [x] `cgroup.stat.local` — local requested-freeze time.
- [x] `cgroup.type` — typed domain/threaded topology.
- [x] `cgroup.procs`, `cgroup.threads` — observed process and thread IDs.
- [x] `cgroup.controllers`, `cgroup.subtree_control` — controller availability and child enablement.
- [x] `cgroup.max.depth`, `cgroup.max.descendants` — subtree limits.
- [x] `cgroup.freeze` — requested freeze state.
- [x] `cgroup.pressure` — configured PSI accounting enablement.
- [x] `irq.pressure` — full-only IRQ pressure snapshot.

## P3 — I/O

- [x] `io.stat` — per-device byte and operation counters keyed by `major:minor`.
- [x] `io.pressure` — shared PSI `some`/`full` averages and total stall time.
- [x] `io.weight` — default and per-device weights.
- [x] `io.max` — per-device bandwidth and IOPS limits.
- [x] `io.latency` — per-device latency targets where supported.
- [x] `io.cost.qos`, `io.cost.model` — root-level I/O cost configuration.
- [x] `io.prio.class` — I/O priority-class policy.
- [x] Documented `io.stat` latency and cost fields.

## P4 — CPU and memory placement

- [x] `cpuset.cpus` — configured CPU ID list.
- [x] `cpuset.cpus.effective` — effective CPU ID list.
- [x] `cpuset.mems` — configured memory-node ID list.
- [x] `cpuset.mems.effective` — effective memory-node ID list.
- [x] `cpuset.cpus.partition` — partition state.
- [x] `cpuset.cpus.exclusive`, `cpuset.cpus.exclusive.effective` — requested and granted exclusive CPUs.
- [x] `cpuset.cpus.isolated` — root isolated CPUs.

## P5 — Event delivery

- [x] Add `poll` support for evented interfaces without changing read
      semantics.
- [x] Add inotify-based change notification for configuration interfaces.

## P6 — Remaining v2 controllers

- [x] `hugetlb.*` — typed per-page-size usage, limit, event, and reservation
      interfaces.
- [x] `rdma.*` — typed per-device RDMA current, peak, maximum, and event resources.
- [x] `dmem.*` — typed device-memory capacity, current, peak, protections, and limits.
- [x] `misc.*` — typed miscellaneous-resource current, maximum, and event
      interfaces.

## P7 — Compatibility and mutation (after read metrics)

- [ ] Add writes for CPU and memory with write-specific input types and
      semantics; do not model writes as the inverse of reads.
- [ ] Add typed cgroup v1/hybrid conversion helpers while keeping v1 units and
      `memory.stat` types distinct from v2.

## Others

- [ ] Duplicate and extra (future) fields (explicitly out-of-scope for now)
- [ ] cgroup v1
- [ ] cgroup v2 write
- [ ] Benchmark
- [x] "Since Linux x.x" documentation — public Rust APIs link to Linux
      references, with mainline introduction versions for interfaces and
      fields added after the Linux 5.15 test baseline.
