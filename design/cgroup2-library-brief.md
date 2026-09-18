# cgroup v2 read library — implementation brief

This document is the handoff from the design sessions. It is meant for a human (or a fresh agent session) to implement the project in **small PRs**. Do not re-litigate the decisions below unless a constraint has changed.

**Product:** a Linux-only, **read-only** Rust library that parses cgroup v2 interface files with typed units (**`uom` + `nutype`**) and a dirfd-based handle, plus a **PyO3** wheel so Python can size workers and memory from `cpu.max` / `memory.max`. First public milestone is **CPU + memory** (plus the core files needed to discover them).

**Non-goals for v0:** writing cgroup files, cgroup v1 managers, creating/deleting cgroups, other language bindings, dual syscall backends, `serde` / `jiff`.

---

## 1. Positioning — is this worth building?

Yes, **if** it stays a typed reader + Python helper. No, if it becomes “another OCI cgroup manager.”

| Existing library                                                   | What it is                                              | Relation                                                             |
| ------------------------------------------------------------------ | ------------------------------------------------------- | -------------------------------------------------------------------- |
| [`cgroupfs`](https://crates.io/crates/cgroupfs) (Facebook `below`) | Read-only cgroup v2, dirfd + parse; `max` as `i64` `-1` | Closest overlap. Weaker units/DX, no Python, tied to `below-common`. |
| [`cgroups-rs`](https://crates.io/crates/cgroups-rs) (Kata)         | Create/delete, v1+v2, systemd, OCI                      | Runtime **manager**, not a typed reader.                             |
| [`libcgroups`](https://crates.io/crates/libcgroups) (youki)        | Runtime manager + BPF devices                           | Same; `oci-spec` in the graph.                                       |
| `procfs` / `sysinfo`                                               | Broad `/proc` and host metrics                          | Incidental cgroup lines, not controller files.                       |
| [`cgroupspy`](https://pypi.org/project/cgroupspy/)                 | Python v1-ish tree, libvirt                             | Stale vs v2.                                                         |
| `psutil`                                                           | Process metrics                                         | Explicitly not a cgroup library.                                     |

If the only consumer were Rust, contributing to `cgroupfs` would be higher leverage. The reason for a new crate is: **spec-faithful types**, **`uom` dimensions** (time vs information vs pages vs event counts), **`nutype` on documented ranges**, **`FileMissing` instead of panics**, and a **Linux PyO3 wheel** for a problem CPython still has: `os.process_cpu_count()` follows **affinity**, not `cpu.max`, and pymalloc does not read `memory.max`.[^cpython-cpu][^go-gomaxprocs]

**Python API product** is helpers like `effective_cpu_count()` and `memory_max()`, not a 1:1 dump of `MemoryStatBytes`.

### Why not Node / Bun / Deno bindings (reassessed)

Backend JS is **not** in Python’s situation. CPython still ignores `cpu.max` / `memory.max` in the **stdlib and allocator**. Node, Deno, and Bun have been teaching the **runtime** those files; remaining holes are version skew and in-runtime bugs, which an N-API addon would not fix (it cannot size V8/JSC at isolate creation).

| Runtime                   | CPU quota (`cpu.max`) for “how many workers?”                                                                                                                                                                                                                                       | Memory limit (`memory.max`) for the heap                                                                                                                                                                      |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **CPython**               | `os.cpu_count()` / `os.process_cpu_count()` = **affinity**, not quota.[^cpython-cpu]                                                                                                                                                                                                | pymalloc / default heap **does not** read cgroup. No `GOMEMLIMIT` analogue.                                                                                                                                   |
| **Node.js**               | `os.cpus().length` = host cores (docs: do **not** size from it). `os.availableParallelism()` → libuv; **cgroup-aware since libuv 1.49** (Node **22.12+**). Node **20** LTS may still ignore quota. Early 1.49 builds had Kubernetes v2 bugs (quota → 1).[^node-uv-cpu][^libuv-4740] | **cgroup v1 since ~12**; **cgroup v2 via libuv since 20.3.0**. Node 18 does **not** reliably see v2 `memory.max`. `process.constrainedMemory()`. Kubernetes documents this.[^k8s-cgroup-v2][^node-cgroup-mem] |
| **Deno**                  | Uses Rust `available_parallelism` (quota-aware when the fs is readable). V8 platform pool capped at 4, like Node.                                                                                                                                                                   | Heap cgroup support landed **2025-05** but is **opt-in** (`DENO_USE_CGROUPS=1`). Default still uses host RAM → OOM in small pods unless the image sets the env.[^deno-cgroup-mem]                             |
| **Bun**                   | Until 2026, `os.availableParallelism()` / `hardwareConcurrency` often returned **host** `sysconf` (Node reported quota). Fix: `WTF::numberOfProcessorCores()` = min(online, affinity, cgroup quota).[^bun-cpu]                                                                      | JSC historically did **not** cap the heap from `memory.max` (OOM 137 in k8s). `process.constrainedMemory()` and GC-vs-cgroup work is in flight; `--max-old-space-size` was ignored for a long time.[^bun-mem] |
| Go 1.25 / Java 10+ / .NET | Runtime reads quota                                                                                                                                                                                                                                                                 | Runtime / GC reads limit                                                                                                                                                                                      |

**Keep Python-only for v0.** A `cgroup2` npm package would be three packaging stories (N-API / Deno FFI / Bun) for APIs the runtimes already expose (`availableParallelism`, `constrainedMemory`). Bun is the closest to Python’s hole, but the fix belongs **in Bun**, not in an addon. Revisit JS only if we want a typed **metrics/ops** reader, not worker-sizing helpers.

---

## 2. Locked decisions (do not reopen in the first PRs)

| Topic            | Decision                                                                                                                                | Brief why                                                                                                                                                                                                                                                               |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Syscall crate    | **`rustix` only**                                                                                                                       | `openat2` + `RESOLVE_BENEATH`, `OwnedFd`, `CGROUP2_SUPER_MAGIC`, `Pid`. Do **not** feature-flag `nix` vs `rustix` (Cargo features are additive; two dependents would pull both). Keep `rustix::process::Pid` **out** of the public API so the backend can change later. |
| Live values      | Treat every read as a **snapshot**                                                                                                      | Gauges/counters move without writes; `cgroup.procs` can tear during one read. Re-read live files. rustdoc `Volatility`; no cache layer in v0.                                                                                                                           |
| Units (Rust)     | **`uom`** for dimensions; **`nutype`** for spec ranges                                                                                  | See §3.5. Time conversions and `Count / Time` → event rate. Pages ≠ bytes until `PAGE_SIZE` is known. `cpu.max.burst` is **time**, same as `$MAX`.                                                                                                                      |
| `serde` / `jiff` | **Not now**                                                                                                                             | Optional later on already-parsed types. Do not use serde to parse cgroup text.                                                                                                                                                                                          |
| Units (Python)   | **`as_microseconds()` / `as_nanoseconds()` / `as_seconds()` always**; **[Pint](https://pint.readthedocs.io/) as extra** `cgroup2[pint]` | Do not pass `uom::Quantity` through PyO3. Wrap in Pint in `python/cgroup2/_units.py`.                                                                                                                                                                                   |
| testcontainers   | **Do not use**                                                                                                                          | E2E needs the host kernel, not a sidecar. `docker run … cargo test` is the Linux runner.                                                                                                                                                                                |
| Other languages  | **Python only**                                                                                                                         | See §1. Node/Deno already consume cgroup in the runtime (with version caveats). Bun is catching up in-tree. No N-API/Deno/Bun, and no Ruby/PHP/Elixir/Swift, in this phase.                                                                                             |
| MSRV             | **Follow PyO3** (plan **1.83+**; **1.85** if edition 2024)                                                                              | rustix’s 1.63 is irrelevant once PyO3 is in the workspace. CI: MSRV job + stable. Bump only in minor releases.                                                                                                                                                          |
| Python           | **3.10+**, `abi3-py310`                                                                                                                 | maturin supports this. 3.10 is near upstream EOL (2026-10-31) but Ubuntu 22.04 still ships it. Linux wheels only.                                                                                                                                                       |

---

## 3. Architecture

### 3.1 Workspace layout (create this on day one)

```text
cgroup2/                          # repo root (name TBD; keep crate name cgroup2)
  Cargo.toml                      # workspace
  rust-toolchain.toml             # stable
  LICENSE
  README.md
  crates/
    cgroup2/                      # rlib → crates.io
      Cargo.toml
      src/
        lib.rs                    # re-exports only
        error.rs
        unit.rs                   # uom aliases, nutype ranges, Count Kind, Pages
        parse.rs                  # pub(crate) format parsers
        pressure.rs               # PSI (cpu + memory)
        mount.rs                  # mountinfo text (any OS)
        proc.rs                   # /proc/pid/cgroup text (any OS)
        handle.rs                 # #[cfg(target_os = "linux")]
        v2/
          mod.rs
          core.rs                 # controllers, subtree_control, type
          cpu.rs
          memory.rs
      tests/
        fixtures/                 # checked-in kernel dumps
        parse_cpu.rs
        parse_memory.rs
        parse_proc.rs
        parse_mount.rs
        linux_live.rs             # cfg(target_os = "linux")
    cgroup2-py/                   # cdylib, publish = false on crates.io
      Cargo.toml
      src/lib.rs
  python/
    cgroup2/
      __init__.py                 # quota helpers
      _units.py                   # as_*() always; to_pint() if extra installed
    tests/
  pyproject.toml                  # maturin, manifest-path = crates/cgroup2-py/Cargo.toml
  devenv.nix / devenv.yaml
  .github/workflows/
    ci.yml
    release.yml                   # tags only
```

**Rules:**

- One crate for Rust logic. Do **not** split `cgroup2-cpu` / `cgroup2-memory`.
- Do **not** use `crate-type = ["rlib", "cdylib"]` on the crates.io crate.
- `cgroup2-py` has `publish = false`.
- Parsers (`unit`, `parse`, `pressure`, `mount`, `proc`) **must compile on macOS**. Kernel I/O is `#[cfg(target_os = "linux")]`.
- Public paths: `cgroup2::{Cgroup, Error, Bytes, Time, Ratio, Pages, Count, MaxOr, CpuWeight, Pressure}` and `cgroup2::v2::{cpu, memory, core}`. Re-export a **short alias** for every `uom` quantity used in the public API — do not leak `Quantity<dyn Dimension, …>` in rustdoc.

### 3.2 Cargo (Linux)

```toml
# crates/cgroup2/Cargo.toml
[package]
name = "cgroup2"
edition = "2024"          # or 2021 if you stay below 1.85
rust-version = "1.85"     # match chosen MSRV
license = "MIT OR Apache-2.0"

[target.'cfg(target_os = "linux")'.dependencies]
rustix = { version = "1", default-features = false, features = ["fs", "process"] }
# later: "event" for poll, inotify feature when events land

[dependencies]
thiserror = "2"
nutype = "0.6"            # pin a current 0.6.x; validate() on documented ranges
uom = { version = "0.37", default-features = false, features = ["u64", "si"] }
# Quantity storage is u64 (kernel integers). Do not enable f32/f64 features.
```

**Do not** add a Cargo feature that swaps `uom` off — the public Rust types _are_ `uom` quantities (behind aliases). Optional `serde` later can sit on those types.

Later rustix features: inotify/poll when `cgroup.events` watchers land — not in v0.

### 3.3 rustix usage (implementation, not public API)

Use:

- `rustix::fs::{openat, openat2, fstatfs, CGROUP2_SUPER_MAGIC, CGROUP_SUPER_MAGIC, TMPFS_MAGIC, OFlags, Mode, ResolveFlags, CWD}`
- `openat2` with `OFlags::RDONLY | CLOEXEC | NOFOLLOW | DIRECTORY` and `ResolveFlags::BENEATH | NO_SYMLINKS` (and `NO_XDEV` if it does not break bind-mounted cgroup roots — verify on Docker).
- Fallback: if `openat2` returns `NOSYS` (kernel &lt; 5.6), `openat` + `O_NOFOLLOW`.
- `rustix::process::{Pid as SysPid, getpid}` — wrap as `cgroup2::Pid(u32)` or `from_raw` internally.
- `rustix::fs::Dir` (or iterate with `openat`) for `children()`.

Do **not** re-export rustix types from `lib.rs`.

### 3.4 Kernel compatibility (runtime, not cfg)

Do not `#ifdef` kernel versions. **Missing file → `Error::FileMissing`.** Unknown keys in flat-keyed files → `extra: BTreeMap<String, u64>`.

| Kernel | What exists (cpu/memory relevant)                        |
| ------ | -------------------------------------------------------- |
| 4.5    | v2 official; memory, io, pids                            |
| 4.14   | threaded mode; `cgroup.stat`                             |
| 4.15   | **cpu** controller (`cpu.stat`, `cpu.max`, `cpu.weight`) |
| 4.20   | PSI (`*.pressure`)                                       |
| 5.6    | `openat2`; hugetlb v2                                    |
| 5.14   | `cgroup.kill` (RHEL 9)                                   |
| 5.15   | Ubuntu 22.04; `cpu.idle` around this era                 |
| 5.19+  | `memory.reclaim`, zswap knobs keep growing               |
| ~6.6+  | `memory.peak` write = per-FD reset (was read-only)       |
| 6.12+  | `cpu.stat.local`, `cgroup.stat.local`                    |

**v0 test floor:** Ubuntu 22.04 / kernel **5.15** = cpu + memory + PSI “fully useful.” Older/hybrid hosts must return `FileMissing`, not panic. Distros may still be hybrid (v2 at `/sys/fs/cgroup/unified` **without** resource controllers).[^systemd-delegation][^cgroups7]

`memory.stat` keys are inserted in the **middle** of the file on new kernels. Parse by **key**, never by line index.[^cgroup-v2]

### 3.5 Units: `uom` + `nutype`

Kernel text is still parsed as integers. The crate then **constructs** `uom` quantities and **validates** closed ranges with `nutype`. Out-of-range values are **parse failures** (`Error::Parse`), and the error must keep the **raw line**.

#### `uom` — dimensions, not pretty-print

Use `uom` with **`u64` storage** (`default-features = false`, `features = ["u64", "si"]`).[^uom] Alias every public quantity:

| Domain                | uom quantity                                         | Canonical unit when parsing kernel text | Notes                                                                                                                                           |
| --------------------- | ---------------------------------------------------- | --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Time                  | `si::time::Time<BaseUnits, u64>` as `Time`     | **microsecond**                         | Kernel default is µs; nanosecond storage also preserves finer I/O times. `MaxOr<Time>` for the `max` token.                     |
| Memory amounts        | `si::u64::Information` as `Bytes`                    | **byte**                                | `memory.current`, `anon`, …                                                                                                                     |
| Percents              | `si::u64::Ratio` as `Ratio`                          | **`part_per_ten_thousand`**             | Kernel `1234` = 12.34%. **Never** `.get::<percent>()` on `u64` — that truncates 12.34 → 12.                                                     |
| Event / period counts | custom **`Count`** (own `Kind`, dimensionless)       | 1                                       | `nr_periods`, `pgfault`, `oom`. **Not** `Ratio` and **not** `Information`.                                                                      |
| Pages                 | custom **`Pages`** (own `Kind`) or a non-uom newtype | 1 page                                  | **Not** `Information`. Convert to `Bytes` only with an explicit `page_size: Information`.                                                       |
| Rates                 | `Count / Time`                                       | —                                       | Dimension T⁻¹. Prefer a distinct **`EventRate` Kind** if mixing with SI `Frequency` (Hz) would confuse rustdoc; otherwise `si::u64::Frequency`. |

**`cpu.max` quota/period** is a **dimensionless duty cycle** (`Time / Time`), not a frequency. **`cpu.max.burst` and `burst_usec` are time** (µs), same dimension as `$MAX` — **not** cycles/s.

`BaseUnits` uses the existing `uom` SI dimensions with nanoseconds as the base time unit (other base units remain SI). `Time` uses `u64` storage, holding about 584 years of accumulated time with nanosecond precision. CPU counters aggregate across CPUs: 1,024 continuously busy CPUs reach this range in about 208 days. This is a practical range limit, not a guarantee for every valid kernel counter. Microsecond parsing must use checked multiplication by 1,000 and return a parse error above 18,446,744,073,709,551 µs; never panic, wrap, or saturate. Do not use `si::u64::Time`: its whole-second storage truncates subsecond values. Conversions to coarser units truncate and finer units can overflow. To compute fractional quota/period ratios, convert the counts to a fractional representation before dividing; integer quantity division truncates.

**Pages ≠ bytes** until `PAGE_SIZE` is known. Do not treat `pswpin` as `Information`. In SI, `uom`’s `Information` is **dimensionless** (kind-separated from `Ratio`); that does **not** make pages interchangeable with bytes.

`to_duration()` on `Time` is optional (`std::time::Duration`); keep `uom` as the public type.

#### `nutype` — documented closed ranges only

Validate **spec ranges**. Do **not** nutype unbounded counters (`memory.current`, `pgfault`).[^nutype]

| Value                        | Range          | Type                           |
| ---------------------------- | -------------- | ------------------------------ |
| `cpu.weight.nice`            | **[-20, 19]**  | `Nice`                         |
| `cpu.idle`, freeze-style 0/1 | **0 or 1**     | `Flag01` or `bool` after parse |
| `cpu.weight` shares          | **[1, 10000]** | `Weight`                       |

**`cpu.weight` exception:** the file is **0 when idle**. Do **not** put `#[nutype(validate(min = 1, max = 10000))]` on the raw file value. Model:

```rust
pub enum CpuWeight {
    Idle,
    Shares(Weight), // nutype 1..=10000
}
```

`ReadCpu::weight()` returns `CpuWeight`, not a bare `u16`.

#### PyO3 boundary

The **cdylib returns POD**: `u64` plus a unit tag (or kernel-native integers). **Do not** expose `uom::Quantity` to Python. Convenience conversions live in Python (`as_microseconds()`, …). Pint wrapping is an **optional extra** (§7).

---

## 4. v0 public interface (CPU + memory only)

Include **discovery, handle, core enablement, cpu, memory, PSI**. Defer io, pids, cpuset, freeze/kill, writes, v1.

### 4.1 Errors and units

Public unit types are the **aliases from §3.5**, not raw `u64` wrappers. Sketch:

```rust
pub enum Error {
    FileMissing { path: PathBuf },
    /// Threaded cgroup: read of cgroup.procs → EOPNOTSUPP
    NotSupported,
    NotCgroupV2,
    DeletedCgroup { path: PathBuf },
    Parse { file: &'static str, detail: String }, // include raw line
    Io(std::io::Error),
}

pub type Time = si::time::Time<BaseUnits, u64>;
pub type Bytes = si::u64::Information;
pub type Ratio = si::u64::Ratio;
pub struct Pages(/* custom Kind or newtype */);
pub struct Count(/* custom Kind */);
pub enum MaxOr<T> { Max, Value(T) }

#[nutype(validate(min = 1, max = 10000))]
pub struct Weight(u16);

#[nutype(validate(min = -20, max = 19))]
pub struct Nice(i8);

pub enum CpuWeight { Idle, Shares(Weight) }

/// Document in rustdoc per file. Not a runtime cache.
pub enum Volatility { Live, Config, Evented, RacySnapshot }
```

Do **not** ship `PercentMilli` or `Micros` as public names. PSI `avg*` fields are `Ratio`; PSI `total` and `cpu.stat` times are `Time`.

### 4.2 Discovery (OS-agnostic parsers + Linux statfs)

Special **absolute** paths:

| Path                             | Role                                                               |
| -------------------------------- | ------------------------------------------------------------------ |
| `/proc/self/mountinfo`           | Find `cgroup2` fstype; magic `0x63677270` (`"cgrp"`) via `fstatfs` |
| `/proc/$PID/cgroup`              | v2 line is always `0::$PATH`; optional suffix ` (deleted)`         |
| `/proc/self/cgroup`              | Same; **cgroupns-virtualized**                                     |
| `/sys/fs/cgroup`                 | Unified: cgroup2. Hybrid: tmpfs of v1 controllers                  |
| `/sys/fs/cgroup/unified`         | systemd hybrid: cgroup2 **without** resource controllers           |
| `/sys/kernel/cgroup/delegate`    | Delegatable file names (defer reading until delegation work)       |
| `/sys/kernel/cgroup/features`    | `nsdelegate`, `memory_localevents`, … (defer)                      |
| `/proc/cgroups`                  | **v1 only**; meaningless for v2 — use `cgroup.controllers` at root |
| `/proc/pressure/{cpu,memory,io}` | System-wide PSI; same text format as cgroup `*.pressure`           |

**Never hard-code `/sys/fs/cgroup` as cgroup2.** Join mountpoint + `0::` path. Inside a cgroup namespace the path is often `/`.[^cgroup-ns]

```rust
pub struct Mount {
    pub target: PathBuf,
    pub root_in_fs: PathBuf,
    pub options: MountOptions, // parse known flags; ignore unknown
}
pub struct ProcCgroupLine {
    pub hierarchy_id: u32,      // 0 = v2
    pub controllers: Vec<String>, // empty on v2
    pub path: PathBuf,
    pub deleted: bool,
}
```

### 4.3 Handle

```rust
pub struct Cgroup { /* OwnedFd dir, PathBuf for Display only */ }

pub trait OpenCgroup: Sized {
    fn from_current_process() -> Result<Self>;
    fn from_pid(pid: u32) -> Result<Self>;
    fn from_path(path: &Path) -> Result<Self>;
    fn child(&self, name: &OsStr) -> Result<Self>;
    fn children(&self) -> Result<Vec<Self>>;
}
```

Reads go through `openat` on the dirfd (TOCTOU). Child names can collide with interface files (`mkdir cpu.stat`); the kernel does not prevent that.[^cgroup-v2]

### 4.4 Core (required even for cpu/memory)

```rust
pub enum CgroupType { Domain, DomainThreaded, DomainInvalid, Threaded }

pub trait ReadCore {
    fn ty(&self) -> Result<CgroupType>;           // cgroup.type (non-root)
    fn controllers(&self) -> Result<Vec<String>>; // cgroup.controllers
    fn subtree_control(&self) -> Result<Vec<String>>;
}
```

These explain why `memory.current` is missing (root exemption, controller not enabled on parent, hybrid). Skip in v0: `cgroup.freeze`, `cgroup.kill`, `cgroup.stat`, `cgroup.events`, `cgroup.max.*`, `irq.pressure`.

### 4.5 PSI (shared)

Same format as `/proc/pressure/*`.[^psi]

```rust
pub struct PressureLine {
    pub avg10: Ratio,
    pub avg60: Ratio,
    pub avg300: Ratio,
    pub total: Time,
}
pub struct Pressure { pub some: PressureLine, pub full: PressureLine }
```

Used by `cpu.pressure` and `memory.pressure`. **Do not write** these files on the read path (a write registers a `poll` trigger on that FD).[^psi]

### 4.6 CPU

```rust
pub struct CpuBandwidthStat {
    pub nr_periods: Count,
    pub nr_throttled: Count,
    pub throttled: Time,
    pub nr_bursts: Count,   // Option-equivalent: extra/absent on old kernels
    pub burst: Time,        // time (µs), not frequency
}
pub struct CpuStat {
    pub usage: Time,
    pub user: Time,
    pub system: Time,
    /// Only when cpu controller is enabled. Non-hierarchical.
    pub bandwidth: Option<CpuBandwidthStat>,
}
pub struct CpuStatLocal { pub throttled: Option<Time> } // ancestor limits included
pub struct CpuMax { pub max: MaxOr<Time>, pub period: Time }

pub trait ReadCpu {
    fn stat(&self) -> Result<CpuStat>;          // exists even if controller off
    fn stat_local(&self) -> Result<CpuStatLocal>; // FileMissing on old kernels
    fn weight(&self) -> Result<CpuWeight>;      // Idle vs Shares(1..=10000)
    fn weight_nice(&self) -> Result<Nice>;
    fn max(&self) -> Result<CpuMax>;
    fn max_burst(&self) -> Result<Time>;        // time, same dimension as $MAX
    fn pressure(&self) -> Result<Pressure>;
    fn uclamp_min(&self) -> Result<Ratio>;
    fn uclamp_max(&self) -> Result<MaxOr<Ratio>>;
    fn idle(&self) -> Result<bool>;
}
```

`cpu.stat` always has `usage_usec` / `user_usec` / `system_usec`. Bandwidth keys appear when the controller is enabled.[^cgroup-v2]

### 4.7 Memory

Split `memory.stat` by unit. Keep unknown keys.

```rust
pub struct MemoryEvents {
    pub low: Count, pub high: Count, pub max: Count,
    pub oom: Count, pub oom_kill: Count,
    pub oom_group_kill: Count, pub sock_throttled: Count,
    // absent keys → 0 or Option; prefer Option for keys that appear later
}
pub struct MemorySwapEvents { pub high: Count, pub max: Count, pub fail: Count }

pub struct MemoryStatBytes { /* anon, file, kernel, sock, zswap, … as Option<Bytes> */ }
pub struct MemoryStatPages { /* pswpin, pgscan, … as Option<Pages>; pgfault as Count */ }
pub struct MemoryStat {
    pub bytes: MemoryStatBytes,
    pub pages: MemoryStatPages,
    pub extra: BTreeMap<String, u64>,
}
pub struct MemoryNumaStat { pub by_type: BTreeMap<String, Vec<(u32, Bytes)>> }

pub trait ReadMemory {
    fn current(&self) -> Result<Bytes>;
    fn min(&self) -> Result<Bytes>;
    fn low(&self) -> Result<Bytes>;
    fn high(&self) -> Result<MaxOr<Bytes>>;
    fn max(&self) -> Result<MaxOr<Bytes>>;
    fn peak(&self) -> Result<Bytes>; // per-FD reset only on new kernels; v0 is read
    fn oom_group(&self) -> Result<bool>;
    fn events(&self) -> Result<MemoryEvents>;
    fn events_local(&self) -> Result<MemoryEvents>;
    fn stat(&self) -> Result<MemoryStat>;
    fn numa_stat(&self) -> Result<MemoryNumaStat>;
    fn swap_current(&self) -> Result<Bytes>;
    fn swap_high(&self) -> Result<MaxOr<Bytes>>;
    fn swap_peak(&self) -> Result<Bytes>;
    fn swap_max(&self) -> Result<MaxOr<Bytes>>;
    fn swap_events(&self) -> Result<MemorySwapEvents>;
    fn zswap_current(&self) -> Result<Bytes>;
    fn zswap_max(&self) -> Result<MaxOr<Bytes>>;
    fn zswap_writeback(&self) -> Result<bool>;
    fn pressure(&self) -> Result<Pressure>;
}
```

Skip `memory.reclaim` (write-only). Root cgroup has **no** `memory.current` / limit files.[^cgroup-v2]

### 4.8 Format parsers (`parse.rs`)

Implement these four v2 formats (plus CPU list later for cpuset):[^cgroup-v2]

1. Single value: `VAL\n` — including `max`
2. Space-separated: `VAL0 VAL1 …\n` (`cpu.max` is two values: `$MAX $PERIOD`)
3. Newline-separated: `cgroup.procs` (defer actual procs read if you skip membership in v0)
4. Flat keyed: `KEY VAL\n`
5. Nested keyed: `KEY sub=val sub=val` (PSI, later io/rdma)

Writable-file write syntax is **not** the inverse of read (defer).

---

## 5. Later extension (do not implement in v0; keep types from blocking it)

**Writes:** same handle and parsers. **Different** write types: `subtree_control` is `+cpu -io`; nested keyed files take **one key per write**; `cpu.max` may omit period; `memory.peak` write is a reset (FD-local); PSI write = monitor; `O_NONBLOCK` on `memory.max`/`high` skips synchronous reclaim.

**v1 / hybrid:** share discovery + parsers + units. **Do not alias** `MemoryStat` to v1 `memory.stat`. v1 `cpuacct.usage` is **nanoseconds**; `cpuacct.stat` is **USER_HZ**; v2 `cpu.stat` is **microseconds** — same `Time` type, **different constructor unit**.[^cpuacct][^cgroup-v1] A controller cannot be in v1 and v2 at once.[^cgroups7]

**After CPU+memory (coarse order):** pids + `cgroup.events` → io → cpuset → inotify/poll → writes for cpu/memory → remaining controllers → v1.

---

## 6. OSS caveats (implement against these)

1. Parse **keys**, never column index; keep `extra` maps.
2. Missing file = controller off / root / hybrid — `FileMissing`, not panic.
3. One file read is not atomic with the next; no fake transactions.
4. `memory.stat` mixing bytes/pages/events is the easiest way to ship wrong metrics. Keep them on **different `uom` Kinds**; never add `Pages` to `Bytes`.
5. `openat` + `O_NOFOLLOW` / `openat2` `BENEATH`; do not follow symlinks out of cgroupfs.
6. cgroupns rewrites `/proc/pid/cgroup` to `/`; join with mountinfo.
7. ` (deleted)` suffix = cgroup rmdir’d under a zombie.
8. Do not write `*.pressure` while reading.
9. Peak reset is per-FD on kernels that support it; reopening is a different view.
10. Hierarchical vs `.local` events; `memory_localevents` changes meaning.
11. Memory charge does **not** follow migrate.[^cgroup-v2]
12. `pids.current` may exceed `pids.max` (when you add pids).
13. Non-normative root cpu/io implicit-leaf behaviour is **not** stable ABI.[^cgroup-v2]
14. Do not copy kernel RST into the crate (GPL docs); reimplement from described formats.
15. Test unified **and** (later) hybrid; more than one kernel series.
16. `cpu.weight` of `0` is **idle**, not an invalid share. `CpuWeight::Idle` vs `Shares(Weight)`.
17. PSI/uclamp percents are **hundredths of a percent**. Construct `Ratio` from `part_per_ten_thousand`, not `percent`.

---

## 7. Python bindings and PyPI

**Layout:** see §3.1. `pyproject.toml` at repo root with maturin `manifest-path = "crates/cgroup2-py/Cargo.toml"`.

**Helpers in `python/cgroup2/__init__.py` (the actual product):**

- `current()` → handle for this process
- `effective_cpu_count()` → from `cpu.max` quota/period (`max` → fall back to affinity / documented policy)
- `memory_max()` → `MaxOr` / `None` if unlimited
- `memory_current()`
- Raise a clear `NotImplementedError` / `OSError` on non-Linux at import or first call

**Units on the Python side (locked):**

- The **cdylib returns POD** (`u64` + unit tag, or kernel-native ints). **Do not** pass `uom::Quantity` through PyO3.
- **Always** expose convenience accessors: `as_microseconds()`, `as_nanoseconds()`, `as_seconds()` for times; analogue `as_bytes()` (and later KiB/MiB if wanted) for memory. Default `pip install cgroup2` must work **without** Pint.
- **Pint is an extra:** `cgroup2[pint]`. Pint is the Python units library (not astropy/unyt).[^pint]
- Wrap in **`python/cgroup2/_units.py`**. Do **not** `py.import("pint")` from Rust unless Pint is a hard dependency (it is **not**).
- `to_pint()` (or equivalent) **only** if the extra is installed; otherwise `ImportError` with an install hint.
- One **module-level `UnitRegistry`** so quantities from different calls mix.

```toml
# pyproject.toml
[project.optional-dependencies]
pint = ["pint>=0.24"]
```

**Wheels:** manylinux **and** musllinux, `x86_64` and `aarch64` only. **No macOS/Windows wheels.** `abi3-py310` so one wheel covers 3.10+. Linux-only Trove classifiers. A Darwin `pip install` from sdist should **fail clearly**, not install a stub that looks like it works. The **default wheel does not vendor Pint**; extras are declared in metadata only.

**PyPI trusted publishing (OIDC)** bound to the **release workflow filename** and GitHub environment `release`. No long-lived `PYPI_TOKEN`.[^pypi-trusted][^maturin-dist]

---

## 8. Testing (Mac devenv + Linux)

macOS has **no** cgroup kernel. Docker Desktop / Podman machine **are** already Linux VMs — do not add a second UTM/NixOS guest. Do **not** use `devenv container build` as the test harness (needs a Linux Nix builder).

**Layer 1 — default, native devenv (every save):** golden fixtures. `cargo test` on Darwin runs **only** parsers. Check in real dumps:

- `cpu.stat` with and without bandwidth keys
- `cpu.max` as `max 100000` and finite quota
- `cpu.weight` `100` → `Shares` and idle `0` → `Idle`; out-of-range `10001` → parse error
- `cpu.weight.nice` `-20` / `19`; out of range → parse error
- PSI `some`/`full` lines; `avg10` stored as `Ratio` (`part_per_ten_thousand`)
- `memory.stat` with a **new key in the middle**
- `memory.current`, `memory.max` = `max`
- `0::/path` and `0::/path (deleted)`
- mountinfo lines for unified vs hybrid

**Layer 2 — Linux host / Ubuntu CI:** `tests/linux_live.rs` under `#[cfg(target_os = "linux")]`. `Cgroup::from_current_process()`, read `cpu.stat` and `memory.current` (assert `memory.current` &gt; 0 if the file exists). **No privilege.** If the host is hybrid and memory is on v1, skip or assert `FileMissing` — do not fail the job until hybrid is in scope.

**Layer 3 — Mac E2E, one engine:**

```bash
docker run --rm -v "$PWD":/src -w /src rust:bookworm cargo test
```

Unprivileged is enough to **read** the container’s own cpu/memory files. Defer `--privileged` until writes/nested cgroups.

**devenv:** rustc, cargo, clippy, rustfmt, rust-analyzer. Script `test` = `cargo test`. Script `test-linux` = docker/podman one-liner; error clearly if neither exists.

**CI:** macOS job = parsers; Ubuntu job = full tree including `linux_live`.

---

## 9. GitHub Actions

**Cache split:** host cargo jobs → [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache). manylinux jobs → `PyO3/maturin-action` with `sccache: true` (rust-cache cannot see `target/` inside the manylinux image). Do **not** combine `mozilla-actions/sccache-action` and rust-cache on the same cargo-test job.

| Job                        | Runner                 | Notes                                                                                       |
| -------------------------- | ---------------------- | ------------------------------------------------------------------------------------------- |
| fmt / clippy / docs        | ubuntu                 | rust-cache                                                                                  |
| `cargo test`               | ubuntu + macos         | parsers on both; live only linux                                                            |
| `linux_live`               | ubuntu                 | can merge into cargo test                                                                   |
| pytest + `maturin develop` | ubuntu                 | after PyO3 exists; default install **without** Pint. Optional env/`.[pint]` for `to_pint()` |
| wheels                     | ubuntu, maturin-action | manylinux + musllinux; `sccache: true`                                                      |
| publish crates.io          | ubuntu, tag `v*`       | environment `release`, `id-token: write`                                                    |
| publish PyPI               | after wheels           | trusted publisher, same environment                                                         |

**Release:** tag `vX.Y.Z` matches crate and wheel. **First** crates.io publish is **manual** (trusted publishing requires an existing crate). Then [`rust-lang/crates-io-auth-action`](https://github.com/rust-lang/crates-io-auth-action) + `cargo publish -p cgroup2`.[^crates-trusted] PyPI: `pypa/gh-action-pypi-publish`. Do not publish `cgroup2-py` to crates.io. Do not publish from PRs. Do not log OIDC tokens.

---

## 10. Fine-grained implementation roadmap

Each item is intended as **one PR** (or a stacked pair: impl + tests). Merge when the “Done when” line is true. Do not start PR _n+1_ until _n_ is on the default branch unless you are pairing and stacking.

### Milestone A — repo that compiles on a Mac

**PR A1 — Workspace skeleton**  
Create the tree in §3.1 with empty `lib.rs` (`//!` crate docs stating read-only v2, Linux I/O, parsers are cross-platform). `LICENSE` MIT OR Apache-2.0. `README` with non-goals. Workspace `Cargo.toml`. **Done when:** `cargo test -p cgroup2` on Darwin is green (zero tests OK).

**PR A2 — Tooling**  
`rust-toolchain.toml`, `.gitignore`, `devenv.nix`/`devenv.yaml` with rustc/cargo/clippy/fmt, scripts `test` and `test-linux` (docker stub can `echo` until Layer 3). **Done when:** `devenv shell` + `cargo fmt` + `clippy -D warnings` work on Darwin.

**PR A3 — CI (Rust only)**  
`.github/workflows/ci.yml`: fmt, clippy, `cargo test` on `ubuntu-latest` and `macos-latest`, rust-cache, `rust-version` MSRV job. No wheels yet. **Done when:** empty crate is green on both OSes.

### Milestone B — parse layer (no syscalls)

**PR B1 — `Error` + units**  
`error.rs`, `unit.rs`: `uom` aliases (`Time`, `Bytes`, `Ratio`), custom `Count`/`Pages` Kinds, `MaxOr`, `nutype` `Weight`/`Nice`, `CpuWeight`. Parse kernel µs into `Time`; parse `1234` into `Ratio` via `part_per_ten_thousand`. Tests: `max` token; 1_000_000 µs → 1 s; 12.34% does **not** truncate to 12; `Nice` rejects 20; `Weight` rejects 0 (idle is the enum, not the nutype). **Done when:** those tests pass on Darwin.

**PR B2 — Single-value and two-value parsers**  
`parse.rs`: `parse_single`, `parse_max_or`, `parse_cpu_max` (`$MAX $PERIOD`). Fixtures: `max\n`, `1048576\n`, `max 100000\n`, `50000 100000\n`. **Done when:** golden tests in `tests/parse_cpu.rs` (even before cpu module).

**PR B3 — Flat keyed**  
`parse_flat_keyed` → `BTreeMap<String, String>` or `u64` with a typed second pass. Fixture: `cpu.stat` with three keys; fixture with a **key inserted in the middle**. **Done when:** lookup by key ignores order.

**PR B4 — Nested keyed (PSI)**  
`pressure.rs` + parser for `some avg10=… total=…`. `avg*` → `Ratio` (`part_per_ten_thousand`); `total` → `Time` (µs). Fixtures from real `cpu.pressure`. **Done when:** `avg10`/`total` round-trip from fixture text without truncating 12.34% to 12.

**PR B5 — `/proc` and mountinfo text**  
`proc.rs`: parse `0::/foo`, v1 lines, `(deleted)`. `mount.rs`: parse `cgroup2` lines from mountinfo (no `statfs` yet). Fixtures for unified and hybrid. **Done when:** table-driven tests cover deleted + hybrid `unified` path.

### Milestone C — Linux handle

**PR C1 — rustix dirfd + magic**  
`handle.rs` `cfg(linux)`: `from_path`, `fstatfs == CGROUP2_SUPER_MAGIC`, `openat2` with NOSYS fallback. Integration test ignored on Darwin (does not compile). **Done when:** opening a temp dir that is **not** cgroup2 returns `NotCgroupV2` (can use `/tmp`).

**PR C2 — Discover live mount**  
Combine mountinfo + `statfs` on candidates (`/sys/fs/cgroup`, `…/unified`). `from_pid` / `from_current_process` using `/proc/self/cgroup`. **Done when:** on Ubuntu CI, `from_current_process()` succeeds **or** returns a documented hybrid error (not a panic).

**PR C3 — `child` / `children`**  
Iterate directory entries; skip `.` `..` and known interface prefixes if you can distinguish dirs vs files (`DT_DIR`). **Done when:** unit test with a fake dir is not required; linux_live can list children of `/` without crashing.

### Milestone D — core + cpu + memory

**PR D1 — `v2::core`**  
Read `cgroup.controllers`, `cgroup.subtree_control`, `cgroup.type` (map `FileMissing` on root for `type`). **Done when:** linux_live prints controllers; fixture tests for the three files.

**PR D2 — `ReadCpu::stat` + `max` + `weight`**  
Typed `CpuStat` / `CpuMax` with `Time`. `weight()` → `CpuWeight`. Bandwidth keys optional. Fixtures + linux_live `stat()`. **Done when:** fixture without `nr_throttled` still parses; `0` → `Idle`; `100` → `Shares`; `10001` is `Parse`.

**PR D3 — Remaining cpu files**  
`weight_nice` (`Nice`), `max_burst` (`Time`), `uclamp.*` (`Ratio`), `idle`, `stat_local`, `pressure`. Each `FileMissing`-tolerant. Out-of-range nice is `Parse`. **Done when:** each has a fixture; live test only requires `stat` + `pressure` if present.

**PR D4 — `memory.current` / `max` / `high` / `min` / `low`**  
`MaxOr<Bytes>`. **Done when:** fixtures for `max` and a numeric limit; live `current` if file exists.

**PR D5 — `memory.stat` + `events`**  
Split bytes/pages/`extra`. Fixture with an invented key in the middle. **Done when:** unknown key appears in `extra` and does not drop `anon`.

**PR D6 — Swap, zswap, peak, numa, pressure, oom.group**  
All `FileMissing`-tolerant. **Done when:** linux_live reads `current` + `stat` when unified; skip zswap if absent.

**PR D7 — Crate docs + rustdoc examples (no_run)**  
Volatility notes on each method. README: how to find your cgroup, hybrid warning. **Done when:** `cargo doc -p cgroup2` has no warnings.

### Milestone E — quality gates for the Rust crate

**PR E1 — `linux_live` always on Ubuntu**  
`cpu.stat.usage > 0` (or ≥ 0 if flaky at t=0 — use `>= 0` and `memory.current` if present). **Done when:** CI Ubuntu fails if unified memory is readable and `current` is 0 **and** that would be a real bug; tune assertions to avoid hybrid false fails.

**PR E2 — `test-linux` docker script**  
Wire devenv `test-linux` to `rust:bookworm`. Document Apple Silicon (`aarch64`). **Done when:** a Mac with Docker can run the script.

**PR E3 — crates.io packaging**  
`Cargo.toml` keywords/categories, exclude fixtures if huge, `README` install snippet. **Do not publish yet.**

### Milestone F — Python

**PR F1 — `cgroup2-py` cdylib stub**  
PyO3 module `cgroup2._native` with `version()`. `publish = false`. **Done when:** `maturin develop` on Ubuntu imports.

**PR F2 — Bind `Cgroup::from_current_process` + `cpu.stat` + `memory.current/max`**  
Minimal pymethods returning POD ints + unit tags. **Done when:** pytest on Ubuntu reads current memory via `as_bytes()`.

**PR F3 — `effective_cpu_count()` / `memory_max()`**  
Pure Python or Rust: `cpu.max` quota/period → `max(1, ceil(quota/period))`; `max` token → documented fallback. Tests with **monkeypatched** fixture text if you expose parse, or Rust unit tests for the quota function. **Done when:** a fixture `50000 100000` → `1` (or `0.5` if you return float — **pick one and document**; for worker pools integer `max(1, floor)` is safer).

**PR F4 — `_units.py` + `as_*` accessors**  
Always-on conversions; no Pint import required. **Done when:** tests call `as_microseconds()` / `as_seconds()` / `as_bytes()` with the extra **not** installed.

**PR F5 — pyproject + classifiers + `requires-python` + `pint` extra**  
Linux-only. abi3-py310. `[project.optional-dependencies] pint = ["pint>=0.24"]`. Lazy `to_pint()` in `_units.py`; one module-level `UnitRegistry`. **Done when:** `pip install .` works without Pint; `pip install .[pint]` makes `to_pint()` return compatible quantities.

**PR F6 — Wheel CI + no Darwin wheels**  
maturin-action, `sccache: true`, x86_64 + aarch64, manylinux + musllinux. **Done when:** artifacts upload on tag **or** on `workflow_dispatch`.

### Milestone G — publish

**PR G1 — `release.yml`**  
Tag `v*`, environment `release`, `id-token: write`. crates.io auth action + `cargo publish -p cgroup2`. PyPI trusted publish from wheel artifacts. **Done when:** dry-run documented; first crates.io publish may be manual.

**PR G2 — First release checklist** (not necessarily code)  
crates.io trusted publisher config, PyPI trusted publisher config, LICENSE headers, SECURITY.md stub.

### Milestone H — after v0.1 (do not pull into v0)

In order: **pids** + `cgroup.events` → **io** (nested keyed + `maj:min`) → **cpuset** (`IdList`) → **inotify/poll** → **writes** for cpu/memory only → hugetlb/rdma/dmem/misc → **v1/hybrid** typed conversion helpers.

---

## 11. Suggested first week (if executing now)

1. Name the repo; `git init`; PR A1–A3.
2. PRs B1–B5 on a Mac without Docker.
3. PRs C1–C2 on Ubuntu CI (or Docker).
4. PRs D1–D2 as the first “this is a cgroup library” demo.
5. Stop and re-read §3.5 + §6 + `memory.stat` units before D5.

---

## 12. Interface files in v0 (relative to a cgroup directory)

**Core:** `cgroup.type`, `cgroup.controllers`, `cgroup.subtree_control`

**CPU:** `cpu.stat`, `cpu.stat.local`, `cpu.weight`, `cpu.weight.nice`, `cpu.max`, `cpu.max.burst`, `cpu.pressure`, `cpu.uclamp.min`, `cpu.uclamp.max`, `cpu.idle`

**Memory:** `memory.current`, `memory.min`, `memory.low`, `memory.high`, `memory.max`, `memory.peak`, `memory.oom.group`, `memory.events`, `memory.events.local`, `memory.stat`, `memory.numa_stat`, `memory.swap.current`, `memory.swap.high`, `memory.swap.peak`, `memory.swap.max`, `memory.swap.events`, `memory.zswap.current`, `memory.zswap.max`, `memory.zswap.writeback`, `memory.pressure`

`devices` and `perf_event` have no v2 knobs (`devices` is BPF). `cgroup.kill` / `memory.reclaim` are write-only.

---

## Footnotes (authoritative sources)

[^cgroup-v2]: Tejun Heo, _Control Group v2_, Linux kernel documentation: <https://docs.kernel.org/admin-guide/cgroup-v2.html> — ABI: formats, conventions, interface files, units (µs default, bytes for memory), `max` token, threaded cgroups, root exemptions, memory ownership, non-normative root behaviour.

[^cgroup-v1]: _Control Groups version 1_ index: <https://docs.kernel.org/admin-guide/cgroup-v1/index.html>; core: <https://docs.kernel.org/admin-guide/cgroup-v1/cgroups.html>; memory (explicitly outdated): <https://docs.kernel.org/admin-guide/cgroup-v1/memory.html>.

[^cpuacct]: _CPU Accounting Controller_: <https://docs.kernel.org/admin-guide/cgroup-v1/cpuacct.html> — `cpuacct.usage` nanoseconds; `cpuacct.stat` USER_HZ.

[^blkio]: _Block IO Controller_: <https://docs.kernel.org/admin-guide/cgroup-v1/blkio-controller.html> — v1 counterpart to `io.stat`.

[^psi]: Johannes Weiner, _PSI - Pressure Stall Information_: <https://docs.kernel.org/accounting/psi.html> — `some`/`full`, avg windows, `total` in µs, poll triggers on write.

[^cgroups7]: `cgroups(7)`: <https://man7.org/linux/man-pages/man7/cgroups.7.html> — v1 vs v2, hybrid mounts, `/proc/pid/cgroup`, `/proc/cgroups`, `/sys/kernel/cgroup/{delegate,features}`, systemd unified vs hybrid paths.

[^cgroup-ns]: `cgroup_namespaces(7)`: <https://man7.org/linux/man-pages/man7/cgroup_namespaces.7.html> — virtualized `/proc/pid/cgroup` and mountinfo.

[^systemd-delegation]: systemd, _Control Group APIs and Delegation_: <https://systemd.io/CGROUP_DELEGATION/> — unified vs hybrid vs legacy mount layout (not kernel ABI, but how machines are mounted).

[^cpython-cpu]: CPython issue: `os.process_cpu_count()` should take cgroups CPU limitations into account: <https://github.com/python/cpython/issues/149452>.

[^go-gomaxprocs]: Go 1.25 container-aware `GOMAXPROCS`: <https://go.dev/blog/container-aware-gomaxprocs> — contrast with Python; do not add a Go binding.

[^k8s-cgroup-v2]: Kubernetes, _About cgroup v2_: <https://kubernetes.io/docs/concepts/architecture/cgroups/> — Node.js reads cgroup v2 memory via libuv from **v20.3.0**; v18 does not reliably.

[^node-cgroup-mem]: Node.js issue “Wrong memory ceiling in cgroup v2”: <https://github.com/nodejs/node/issues/47259>; Red Hat, _Node.js 20+ memory management in containers_: <https://developers.redhat.com/articles/2025/10/10/nodejs-20-memory-management-containers>.

[^node-uv-cpu]: libuv 1.49.0: `uv_available_parallelism` uses cgroup; Node `os.availableParallelism()` is a thin wrapper. Docs: <https://nodejs.org/api/os.html#osavailableparallelism>.

[^libuv-4740]: libuv #4740 / Node #58428: cgroup v2 Kubernetes mis-reported parallelism as 1 on some 1.49.1 / Node 22.12 builds.

[^deno-cgroup-mem]: Deno #29077 / PR #29078 (merged 2025-05): V8 heap from cgroup v1/v2, gated on `DENO_USE_CGROUPS`. Official images may set the env; default CLI does not.

[^bun-cpu]: Bun #29129 / PR #28801: cgroup-aware `os.availableParallelism` / `hardwareConcurrency` via WebKit `numberOfProcessorCores()`.

[^bun-mem]: Bun #17723 / PR #29408 (cgroup-aware GC / `constrainedMemory`); #34917 / #34924 (`--max-old-space-size` actually bounding JSC).

[^maturin-dist]: Maturin distribution / abi3 / trusted publishing: <https://www.maturin.rs/distribution.html>; maturin-action: <https://github.com/PyO3/maturin-action>.

[^crates-trusted]: crates.io trusted publishing: <https://crates.io/docs/trusted-publishing>; <https://github.com/rust-lang/crates-io-auth-action>.

[^pypi-trusted]: PyPI trusted publishers: <https://blog.pypi.org/posts/2023-04-20-introducing-trusted-publishers/>.

[^rustix]: rustix: <https://github.com/bytecodealliance/rustix>; docs: <https://docs.rs/rustix/>.

[^cgroupfs]: `cgroupfs` crate (read-only v2, closest overlap): <https://crates.io/crates/cgroupfs>.

[^uom]: `uom` (type-safe units, `u64` + SI): <https://docs.rs/uom/>; crate: <https://crates.io/crates/uom>. Use `Quantity` aliases in the public API.

[^nutype]: `nutype` (newtypes with validation): <https://docs.rs/nutype/>; crate: <https://crates.io/crates/nutype>. Closed ranges only; idle `cpu.weight` is an enum, not a 1..=10000 newtype on the raw file.

[^pint]: Pint (Python units): <https://pint.readthedocs.io/>. Optional extra `cgroup2[pint]`; not a default dependency.

[^python-eol]: CPython branch status / 3.10 EOL: <https://devguide.python.org/versions/>.
