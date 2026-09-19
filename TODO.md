# cgroup v2 metrics TODO

Implement the read-only cgroup v2 surface in priority order. Finish all CPU
items before starting memory; work below memory stays out of the v0 scope.

An interface item is complete when it has a typed public reader, key-based
parsing where applicable, fixtures for supported and missing/older-kernel
forms, documented volatility and units, and a Linux live test where the file
is expected to exist. Missing optional files must return `FileMissing`, and
unknown keyed fields must be preserved when the public type provides an
`extra` map.

## P0 — CPU (complete first)

- [x] `cpu.stat` — usage, user, system, and optional bandwidth counters; parse
      times as microseconds and tolerate bandwidth keys missing when the
      controller is disabled.
- [x] `cpu.max` — quota and period as `MaxOr<NonZeroTime>` and `NonZeroTime`;
      retain the existing typed parser and add the cgroup reader/live coverage.
- [x] `cpu.pressure` — shared PSI `some`/`full` averages and total stall time;
      never write to the file on the read path.
- [x] `cpu.weight` — map `0` to `CpuWeight::Idle` and `1..=10000` to
      `CpuWeight::Shares(Weight)`.
- [ ] `cpu.stat.local` — local throttled time, including its older-kernel
      `FileMissing` behavior.
- [ ] `cpu.uclamp.max` — `max` or a ratio expressed in hundredths of a percent.
- [ ] `cpu.uclamp.min` — ratio expressed in hundredths of a percent.
- [ ] `cpu.max.burst` — burst duration in microseconds, not a frequency.
- [ ] `cpu.weight.nice` — validated nice value in `-20..=19`.
- [ ] `cpu.idle` — boolean idle scheduling state.

## P1 — Memory (start only after P0)

- [ ] `memory.current` — current hierarchical memory usage in bytes.
- [ ] `memory.max` — hard limit as `MaxOr<Bytes>`.
- [ ] `memory.high` — throttling limit as `MaxOr<Bytes>`.
- [ ] `memory.events` — hierarchical low/high/max/OOM counters, preserving
      version-dependent optional keys.
- [ ] `memory.stat` — split byte, page, and count fields into distinct typed
      groups and retain unknown keys in `extra`.
- [ ] `memory.pressure` — shared PSI `some`/`full` averages and total stall
      time; never write to the file on the read path.
- [ ] `memory.peak` — peak usage in bytes; document that reset semantics are
      per file descriptor on kernels that support writes.
- [ ] `memory.low` — best-effort memory protection in bytes.
- [ ] `memory.min` — hard memory protection in bytes.
- [ ] `memory.oom.group` — boolean group OOM policy.
- [ ] `memory.events.local` — local, non-hierarchical memory event counters.
- [ ] `memory.swap.current` — current swap usage in bytes.
- [ ] `memory.swap.max` — hard swap limit as `MaxOr<Bytes>`.
- [ ] `memory.swap.events` — swap high/max/fail counters.
- [ ] `memory.swap.high` — swap throttling limit as `MaxOr<Bytes>`.
- [ ] `memory.swap.peak` — peak swap usage in bytes.
- [ ] `memory.numa_stat` — per-memory-type, per-NUMA-node byte totals.
- [ ] `memory.zswap.current` — current zswap memory usage in bytes.
- [ ] `memory.zswap.max` — zswap limit as `MaxOr<Bytes>`.
- [ ] `memory.zswap.writeback` — boolean zswap writeback policy.

The root cgroup lacks `memory.current` and the memory limit files. Treat that
as `FileMissing`, not a parse failure or panic.

## P2 — Process and cgroup state

- [ ] `pids.current` — current process count; it may temporarily exceed the
      configured maximum.
- [ ] `pids.max` — process limit as `MaxOr<Count>`.
- [ ] `pids.events` — hierarchical process-limit event counters.
- [ ] `cgroup.events` — populated and frozen state for lifecycle monitoring.
- [ ] `cgroup.stat` — descendant and subsystem state counters.

## P3 — I/O

- [ ] `io.stat` — per-device byte and operation counters keyed by `major:minor`.
- [ ] `io.pressure` — shared PSI `some`/`full` averages and total stall time.
- [ ] `io.weight` — default and per-device weights.
- [ ] `io.max` — per-device bandwidth and IOPS limits.
- [ ] `io.latency` — per-device latency targets where supported.

## P4 — CPU and memory placement

- [ ] `cpuset.cpus` — configured CPU ID list.
- [ ] `cpuset.cpus.effective` — effective CPU ID list.
- [ ] `cpuset.mems` — configured memory-node ID list.
- [ ] `cpuset.mems.effective` — effective memory-node ID list.
- [ ] `cpuset.cpus.partition` — partition state.

## P5 — Event delivery

- [ ] Add `poll` support for evented interfaces without changing read
      semantics.
- [ ] Add inotify-based change notification for configuration interfaces.

## P6 — Remaining v2 controllers

- [ ] `hugetlb.*` — typed per-page-size usage, limit, event, and reservation
      interfaces.
- [ ] `rdma.*` — typed per-device RDMA current and maximum resources.
- [ ] `dmem.*` — typed device-memory capacity and usage interfaces.
- [ ] `misc.*` — typed miscellaneous-resource current, maximum, and event
      interfaces.

## P7 — Compatibility and mutation (after read metrics)

- [ ] Add writes for CPU and memory with write-specific input types and
      semantics; do not model writes as the inverse of reads.
- [ ] Add typed cgroup v1/hybrid conversion helpers while keeping v1 units and
      `memory.stat` types distinct from v2.

## Others

- [ ] cgroup v1
- [ ] cgroup v2 write
- [ ] Benchmark
- [ ] E2E tests
- [ ] "Since Linux x.x" documentation
