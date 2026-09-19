# Cross-Platform Process Resource Introspection Research

## Ecosystem Comparison

The motivating problem is not merely whether a language runtime is _internally aware_ of containers or cgroups. The important question is whether an application can reliably discover the **effective resource constraints that apply to its own process**, and use those values when making decisions or spawning subprocesses.

| Ecosystem                  | Similarity to motivating problem                              | Commonly used for scripting / subprocess orchestration | Runtime container / cgroup awareness                         | Good application-facing API for effective limits? | Rust interop                     | Existing relevant solutions / notes                                                                                                                                                                                                                     |
| -------------------------- | ------------------------------------------------------------- | -----------------------------------------------------: | ------------------------------------------------------------ | ------------------------------------------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Python**                 | **Very high**                                                 |                                          **Very high** | Partial                                                      | **No general coherent API**                       | **Excellent — PyO3**             | CPython has had cgroup-related CPU-count issues. `loky.cpu_count()` explicitly considers cgroup restrictions. `psutil` exposes extensive system/process information but not a unified effective-cgroup-limit abstraction.                               |
| **Ruby**                   | **Very high**                                                 |                                               **High** | Limited                                                      | **No**                                            | **Excellent — Magnus / rb-sys**  | Ruby has had requests for `Etc.nprocessors` to respect cgroup limits. Libraries such as `maxprocs-ruby` and `concurrent-ruby` address CPU-count-related cases, but not general process constraints.                                                     |
| **PHP**                    | **High**                                                      |                                                 Medium | Limited                                                      | **No**                                            | **Good — ext-php-rs**            | PHP's `memory_limit` is an application/runtime configuration value, not discovery of the kernel/container boundary. Container-memory-awareness problems have been raised independently.                                                                 |
| **Elixir / Erlang (BEAM)** | **Medium–high**                                               |                                                   High | Partial                                                      | Limited                                           | Possible through NIFs / Rustler  | BEAM applications frequently supervise external processes. MuonTrap provides cgroup-backed process management/resource statistics, but this is oriented toward managing child processes rather than answering “what effective constraints apply to me?” |
| **Perl**                   | **Medium**                                                    |                                          **Very high** | Limited                                                      | **No**                                            | Good through C ABI / XS / FFI    | Third-party cgroup support exists, including Mojo-related tooling, but there is no obvious canonical effective-resource-limit API.                                                                                                                      |
| **Deno**                   | **Medium**                                                    |                                                   High | Partial/runtime-dependent                                    | **No general resource-constraint API**            | **Good — C ABI + `Deno.dlopen`** | Deno has first-class FFI for native dynamic libraries. Native Rust extensions are also possible through Deno internals, but FFI provides a simpler and more stable boundary for an external library.                                                    |
| **Node.js**                | **Medium**                                                    |                                          **Very high** | **Substantial, but incomplete from application perspective** | Limited                                           | Excellent through N-API or C ABI | V8/libuv account for some container constraints, especially memory. CPU-count/cgroup behavior has historically been inconsistent. Runtime awareness does not expose a general cgroup/resource introspection API to applications.                        |
| **Java**                   | Low                                                           |                                                 Medium | **Strong**                                                   | Partial/good through runtime APIs                 | JNI / Panama possible            | Modern JVMs have substantial container awareness. This substantially reduces the original motivation, although a richer portable resource-introspection API could still expose information beyond JVM ergonomics.                                       |
| **.NET**                   | Low                                                           |                                                 Medium | **Strong**                                                   | Partial/good                                      | C ABI / P/Invoke                 | Modern .NET has significant container/cgroup awareness. Less affected by the original problem.                                                                                                                                                          |
| **Go**                     | Low                                                           |                                            Medium–high | Increasingly strong                                          | Partial                                           | C ABI/cgo possible but awkward   | Modern Go has increasingly incorporated container-aware runtime behavior. The problem exists less strongly at the runtime level than in Python/Ruby/PHP.                                                                                                |
| **Rust**                   | Low as a target language; **primary implementation language** |                                                 Medium | Library-dependent                                            | No standard unified abstraction                   | Native                           | Rust provides excellent access to OS facilities and is well suited to implementing the core. Lack of runtime magic is not particularly problematic because systems programmers generally expect explicit OS/library APIs.                               |
| **C / C++**                | Low                                                           |                                                 Medium | Application/library-dependent                                | No standard unified abstraction                   | Native C ABI                     | Similar to Rust: direct OS access is expected, so the motivating ergonomic failure is less pronounced. A C ABI remains useful as a universal interoperability layer.                                                                                    |

The strongest initial language targets therefore appear to be:

**Python → Ruby → Deno → PHP**, with the ordering primarily reflecting ecosystem fit and distribution ergonomics rather than technical feasibility.

The project should nevertheless be designed as a **Rust library first**, with language bindings layered over a stable semantic model.

---

# 1. Problem Statement

Applications frequently need to answer questions such as:

- How much memory can this process actually use?
- How much CPU capacity can it actually consume?
- Which CPUs may it execute on?
- Is it constrained by a container, service manager, job, or process-level limit?
- Is the machine currently under CPU, memory, or I/O pressure?
- Is the process isolated from the host through namespaces or another mechanism?
- Which kernel or operating-system mechanism produced a particular constraint?

Traditional system-information APIs often answer a different question:

> What resources does the machine have?

For containerized, sandboxed, service-managed, or otherwise constrained processes, that may be very different from:

> What resources can **this process** actually use?

For example, a machine may physically contain 64 CPUs and 256 GiB of RAM while a process is restricted to approximately 2 CPUs and 4 GiB.

This distinction becomes particularly important in runtimes such as Python that are commonly used to:

- size worker pools;
- configure scientific workloads;
- launch subprocesses;
- choose concurrency levels;
- select memory/cache sizes;
- orchestrate native tools.

A runtime being internally cgroup-aware does **not** necessarily solve this problem. Applications need an explicit introspection API.

---

# 2. Proposed Scope

The project should not expose **cgroups** as its primary abstraction.

Cgroups are one operating-system implementation mechanism. They should instead be treated as one **provider of process constraints**.

The long-term project can be described as:

> A portable library for discovering the execution environment and effective resource constraints of a process.

The major semantic areas are:

1. **Capacity**
2. **Constraints / limits**
3. **Pressure**
4. **Affinity**
5. **Isolation**
6. **Platform-specific details and provenance**

A possible conceptual API is:

```rust
let env = sysprobe::current()?;

let memory_limit = env.resources().memory().limit();
let cpu_capacity = env.resources().cpu().capacity();
let memory_pressure = env.resources().memory().pressure();

let cpus = env.affinity().cpus();
let isolation = env.isolation();
```

The exact API is not decided, but the important design principle is that callers primarily interact with **semantic concepts**, rather than Linux filesystem paths or Windows API structures.

---

# 3. Capacity vs. Constraints vs. Pressure

These concepts should remain explicitly separate.

## Capacity

Capacity describes resources that exist or are visible.

Examples:

```text
host physical CPUs
host logical CPUs
host physical memory
host swap
visible CPUs
```

Capacity alone does not imply that the process may consume all of it.

---

## Constraints

Constraints describe restrictions placed on the process.

Examples:

```text
memory maximum
memory soft/high threshold
CPU quota
allowed CPUs
maximum number of processes
address-space limit
open-file limit
```

There may be multiple simultaneous sources of constraints.

For example:

```text
Host CPUs:             64
CPU affinity:           8 CPUs
cgroup cpuset:          4 CPUs
cgroup CPU quota:       2.5 CPUs
```

These values should not be collapsed into a single ambiguous `cpu_count`.

---

## Pressure

Pressure describes current resource contention.

It is neither capacity nor a limit.

Linux PSI, for example, can indicate that workloads are spending significant time stalled because CPU, memory, or I/O resources are unavailable.

A machine can therefore simultaneously have:

```text
Memory capacity:   128 GiB
Memory limit:        8 GiB
Memory usage:        6 GiB
Memory pressure:    high
```

These are four different observations.

---

# 4. Effective Constraints

A particularly valuable abstraction is the concept of an **effective constraint**.

Multiple mechanisms can constrain the same resource.

On Linux, memory or CPU behavior may be influenced by:

- cgroups;
- rlimits;
- CPU affinity;
- cpusets;
- service managers;
- namespaces;
- possibly additional platform-specific mechanisms.

On Windows, restrictions may come from:

- Job Objects;
- CPU Sets;
- process affinity;
- process-level APIs;
- nested jobs.

The library can resolve these sources into semantic answers while retaining their provenance.

For example:

```rust
struct EffectiveLimit<T> {
    value: Option<T>,
    sources: Vec<LimitSource<T>>,
}
```

Conceptually:

```text
effective CPU availability
    ├── host capacity:       32 CPUs
    ├── process affinity:     8 CPUs
    ├── cgroup cpuset:        4 CPUs
    └── cgroup quota:         2 CPUs
```

The library should avoid pretending that all of these mechanisms have identical semantics.

In particular:

> **CPU quota is not CPU affinity.**

A process permitted to execute on eight CPUs but limited to two CPU-seconds of execution per second is not equivalent to a process pinned to exactly two CPUs.

Both facts should remain observable.

---

# 5. Provenance

Normalized APIs should not hide where information came from.

A caller may want to distinguish:

```text
cgroup v2
cgroup v1
RLIMIT
sched affinity
Windows Job Object
Windows CPU Set
unconstrained host capacity
```

This is particularly important for:

- debugging;
- observability;
- unexpected container behavior;
- compatibility diagnostics;
- runtime/library authors.

A normalized value could therefore optionally expose something conceptually similar to:

```rust
pub struct Constraint<T> {
    pub value: T,
    pub source: ConstraintSource,
}
```

with:

```rust
pub enum ConstraintSource {
    CgroupV1,
    CgroupV2,
    Rlimit,
    CpuAffinity,
    WindowsJobObject,
    WindowsCpuSet,
    PlatformSpecific,
}
```

More complex constraints may need multiple contributing sources rather than exactly one source.

---

# 6. Linux Backend

Linux should be the initial implementation target.

The Linux backend should eventually combine several independent mechanisms.

## 6.1 cgroup v2

Important CPU files include:

```text
cpu.max
cpu.weight
cpuset.cpus
cpuset.cpus.effective
```

Important memory files include:

```text
memory.current
memory.max
memory.high
memory.low
memory.min
memory.swap.current
memory.swap.max
```

Process-count information includes:

```text
pids.current
pids.max
```

I/O information includes files such as:

```text
io.max
io.weight
io.stat
```

Other useful controller-specific files may be exposed as the scope grows.

The library should not assume that every controller is enabled.

It should also distinguish:

```text
file unavailable
controller unavailable
unlimited
permission denied
malformed/unexpected kernel value
```

where doing so is useful.

---

# 7. cgroup v1

cgroup v1 support is useful for:

- older Linux distributions;
- legacy enterprise environments;
- older container environments;
- compatibility testing.

The public semantic API should preferably make cgroup v1 versus v2 mostly irrelevant to ordinary callers.

For example:

```rust
env.resources().memory().limit()
```

could work regardless of whether the source is:

```text
cgroup v1 memory.limit_in_bytes
```

or:

```text
cgroup v2 memory.max
```

while provenance still exposes the underlying mechanism.

Because v1 and v2 semantics are not universally identical, normalization should be conservative rather than pretending they are perfectly interchangeable.

---

# 8. POSIX/Linux Resource Limits

The library should inspect process resource limits through facilities such as:

```text
getrlimit()
prlimit()
```

Potentially relevant limits include:

```text
RLIMIT_AS
RLIMIT_CPU
RLIMIT_DATA
RLIMIT_FSIZE
RLIMIT_MEMLOCK
RLIMIT_NOFILE
RLIMIT_NPROC
RLIMIT_RSS
RLIMIT_STACK
```

Not every limit maps cleanly to the portable resource model.

For example:

```text
RLIMIT_AS
```

is an address-space constraint and should not simply be treated as equivalent to a cgroup physical-memory limit.

Platform-specific details should remain accessible when no safe normalized interpretation exists.

---

# 9. CPU Affinity

Linux CPU affinity can be queried using mechanisms such as:

```text
sched_getaffinity()
```

This answers:

> Which logical CPUs may this process execute on?

That is distinct from:

> How much CPU time may this process consume?

For example:

```text
Affinity:  CPUs 0-7
Quota:     200000 / 100000
```

could mean that the process may run on eight CPUs but consume approximately two CPUs worth of aggregate CPU time.

The API should preserve both facts.

---

# 10. CPU Capacity Semantics

A single function named something like:

```rust
cpu_count()
```

is likely insufficient.

Useful concepts include:

```text
host logical CPU count
visible CPU count
affinity CPU count
cpuset CPU count
CPU quota
effective parallelism estimate
CPU weight/share
```

A higher-level helper could eventually provide a recommended concurrency value, but it should not replace access to the underlying facts.

For example:

```rust
cpu.parallelism_hint()
```

could potentially derive a worker-count recommendation.

Such a function would be an explicit policy layer rather than the fundamental representation.

---

# 11. Linux Pressure Stall Information

Linux PSI provides pressure information for:

```text
CPU
memory
I/O
```

Common sources include:

```text
/proc/pressure/cpu
/proc/pressure/memory
/proc/pressure/io
```

and cgroup-scoped pressure files where available.

PSI exposes stall metrics rather than limits.

The API should therefore model pressure independently:

```rust
env.pressure().cpu()
env.pressure().memory()
env.pressure().io()
```

or as part of resource-specific observations:

```rust
env.resources().memory().pressure()
```

The exact hierarchy remains an API-design question.

---

# 12. Linux Namespaces

Linux namespaces provide useful information about isolation.

Relevant namespace types include:

```text
PID
mount
network
user
UTS
IPC
cgroup
time
```

Namespace inspection can help describe the process environment, but it should **not** be treated as a reliable universal “am I inside Docker?” mechanism.

Container detection is inherently less reliable than reading the actual kernel-enforced constraints.

The library should prefer:

> What restrictions actually apply?

over:

> Which container product appears to have created this process?

---

# 13. Container Runtime Detection

Possible environments include:

```text
Docker
Podman
containerd
CRI-O
Kubernetes
systemd-nspawn
LXC/LXD
others
```

Detection can rely on hints such as:

- cgroup paths;
- environment variables;
- runtime-created files;
- mount information;
- namespace relationships;
- OCI metadata where available.

However, these are frequently implementation details and may change.

Therefore container identity should be:

```text
optional
best-effort
informational
```

rather than fundamental to resource discovery.

A process does not need to know that it is “inside Docker” to determine that:

```text
memory.max = 4 GiB
cpu.max = 200000 100000
```

apply to it.

---

# 14. systemd

systemd may impose resource constraints using cgroups.

The library should generally observe the resulting kernel state rather than depend on systemd APIs.

This avoids coupling the core implementation to:

- systemd;
- D-Bus;
- specific service managers.

If useful, systemd metadata may later be exposed as additional provenance.

The effective kernel constraint remains the authoritative resource boundary.

---

# 15. Windows Backend

Windows should eventually implement the same **semantic model**, but should not attempt to imitate Linux implementation details.

The closest analogue to Linux cgroups is generally the **Windows Job Object** system.

---

# 16. Windows Job Objects

Job Objects can group processes and impose resource controls.

Depending on Windows version and configuration, relevant capabilities include:

- process memory limits;
- job-wide memory limits;
- CPU time limits;
- CPU rate control;
- active process count limits;
- process affinity;
- scheduling restrictions;
- working-set restrictions;
- accounting information.

This makes Job Objects a natural Windows provider for:

```text
constraints
accounting
isolation metadata
```

However, Linux cgroups and Windows Job Objects are not semantically identical.

The public API should therefore normalize only concepts with sufficiently compatible meanings.

---

# 17. Nested Windows Jobs

Modern Windows permits nested Job Objects in many scenarios.

A process may therefore inherit restrictions from multiple levels.

This reinforces the usefulness of the effective-constraint abstraction.

Conceptually:

```text
Host
└── Outer Job
    └── Inner Job
        └── Process
```

The process may be affected by restrictions imposed at several levels.

The backend should determine the constraints that actually apply rather than merely reporting one arbitrary Job Object.

---

# 18. Windows CPU Sets

Windows CPU Sets provide another mechanism for controlling which processors are available to workloads.

They are conceptually closer to:

```text
cpuset / affinity
```

than to:

```text
CPU quota
```

The Windows backend may therefore combine information from:

```text
Job Objects
CPU Sets
process affinity
system topology
```

to construct the portable CPU model.

Again:

```text
allowed CPUs
```

and:

```text
allowed CPU consumption
```

must remain distinct.

---

# 19. Platform Parity

Linux and Windows should not be forced into fake feature parity.

A portable query should be able to return:

```rust
Option<T>
```

or an explicit unsupported/unavailable state when a concept does not exist on a platform.

For example, a Linux-specific I/O controller concept may have no sufficiently equivalent Windows representation.

It is preferable to expose:

```text
unsupported
```

than to manufacture misleading semantics.

The API can therefore contain both:

```text
portable semantic observations
```

and:

```text
platform-specific extensions
```

---

# 20. Suggested Internal Architecture

A possible crate layout is:

```text
sysprobe/
├── sysprobe-core
├── sysprobe-linux
├── sysprobe-windows
├── sysprobe-python
├── sysprobe-ruby
├── sysprobe-php
└── sysprobe-ffi
```

This is illustrative rather than a finalized workspace design.

A simpler initial workspace could be:

```text
sysprobe/
├── crates/
│   ├── sysprobe/
│   ├── sysprobe-linux/
│   └── sysprobe-python/
```

and split further only when necessary.

The core principle is:

> Platform discovery and language bindings should not define the semantic model.

The Rust data model should remain usable directly by native Rust applications.

---

# 21. Raw vs. Portable APIs

Two levels of API are desirable.

## Portable API

Example:

```rust
let env = sysprobe::current()?;

env.memory().limit();
env.cpu().quota();
env.cpu().affinity();
env.processes().limit();
```

This answers semantic questions.

---

## Platform API

Example:

```rust
env.platform().linux().cgroup_v2();
```

or:

```rust
sysprobe::linux::current_cgroup()
```

This exposes details such as:

```text
cgroup hierarchy
controller availability
raw kernel values
cgroup paths
rlimits
namespace identifiers
```

On Windows:

```rust
env.platform().windows().job();
```

could expose Job Object details.

This separation prevents the portable API from becoming a thin wrapper around whichever operating system was implemented first.

---

# 22. Data Model Considerations

Binding-facing Rust structures should preferably consist of:

- plain structs;
- enums;
- integer values;
- durations;
- byte sizes;
- `Option<T>`;
- vectors;
- sets/maps where appropriate.

Avoid requiring language bindings to expose:

- trait objects;
- complex lifetimes;
- callbacks;
- Rust-specific ownership abstractions;
- self-referential structures.

For example:

```rust
pub struct CpuConstraints {
    pub quota: Option<CpuQuota>,
    pub weight: Option<u64>,
    pub affinity: CpuSet,
}
```

and:

```rust
pub struct MemoryConstraints {
    pub max: Option<ByteSize>,
    pub high: Option<ByteSize>,
    pub swap_max: Option<ByteSize>,
}
```

This keeps language bindings mechanical.

---

# 23. Python Interoperability

Python is the strongest initial non-Rust target.

The recommended stack is:

```text
Rust core
   ↓
PyO3
   ↓
Python extension
```

Packaging can use tools such as Maturin to produce Python wheels.

Typical mappings are straightforward:

```text
Rust                   Python

Option<T>              T | None
Vec<T>                 list[T]
bool                   bool
String                 str
integer                int
struct                 Python class
enum                   enum/class representation
```

Python is especially compelling because:

- it is heavily used as an orchestration language;
- scientific workloads frequently launch native subprocesses;
- worker-pool sizing matters;
- memory limits matter;
- containerized Python workloads are common;
- stdlib APIs do not provide a coherent portable process-resource model.

---

# 24. Ruby Interoperability

Rust-to-Ruby interoperability is mature enough for a first-class binding.

The primary high-level option is:

```text
Magnus
```

with:

```text
rb-sys
```

providing lower-level Ruby API bindings and build infrastructure.

A possible architecture is:

```text
sysprobe-core
    ↓
Magnus
    ↓
Ruby native extension
```

Magnus provides abstractions for:

- defining Ruby classes/modules;
- exporting Rust functions;
- converting Ruby values;
- constructing Ruby values;
- error translation;
- native extension initialization.

`rb-sys` handles lower-level Ruby C API integration and supports extension-building workflows.

An important technical caveat is Ruby's garbage collector.

Rust's borrow checker cannot enforce Ruby GC rooting rules. Ruby objects retained by native code must remain correctly rooted/reachable.

This should have limited impact on this project because the ideal binding architecture is:

```text
query Rust
→ obtain owned Rust data
→ convert data into Ruby values
→ return
```

The core library should not need to retain arbitrary Ruby objects.

Therefore the Ruby binding can remain thin.

---

# 25. PHP Interoperability

The strongest Rust-native option is:

```text
ext-php-rs
```

It provides abstractions over the PHP/Zend extension API and allows PHP extensions to be implemented in Rust.

Relevant functionality includes:

- exposing Rust functions;
- exposing Rust-backed classes;
- Zend value conversion;
- `IntoZval`;
- `FromZval`;
- extension initialization macros.

The architecture can be:

```text
sysprobe-core
    ↓
ext-php-rs
    ↓
PHP native extension
```

The main difficulty is likely **distribution**, not Rust/PHP interoperability.

PHP native extensions depend on factors including:

- PHP version;
- platform;
- architecture;
- Zend ABI;
- build configuration.

Users may require appropriate PHP development files or prebuilt compatible extension binaries.

This is less convenient than Python's wheel ecosystem.

Technically, however, sysprobe's API shape is a good fit because it primarily returns structured data rather than requiring complicated callbacks between PHP and Rust.

---

# 26. Deno Interoperability

Deno provides first-class native FFI through:

```text
Deno.dlopen
```

A Rust library can export a C ABI:

```rust
#[no_mangle]
pub extern "C" fn sysprobe_foo(...) -> ... {
    // ...
}
```

and be compiled as a:

```toml
crate-type = ["cdylib"]
```

Deno can then dynamically load the library.

The architecture is:

```text
sysprobe-core
    ↓
sysprobe-ffi
    ↓
stable C ABI
    ↓
Deno.dlopen
    ↓
TypeScript wrapper
```

This is probably preferable initially to coupling the project directly to Deno's internal Rust extension interfaces.

Deno also has native Rust extension mechanisms through its Rust crates and op system, but those are more tightly coupled to Deno internals.

For an independently distributed library, a C ABI is simpler and more broadly reusable.

One important operational property is that Deno FFI requires the appropriate FFI permission, normally through:

```text
--allow-ffi
```

Native code can escape Deno's normal JavaScript sandbox, so Deno treats FFI as a privileged capability.

---

# 27. Universal C ABI

A small C ABI is worth considering even if Python and Ruby use their ecosystem-specific Rust bindings.

It can become a universal compatibility layer for:

```text
Deno
Node.js
Perl
Lua
R
C
C++
other FFI-capable languages
```

The architecture could therefore become:

```text
                    ┌── PyO3 ─────── Python
                    │
                    ├── Magnus ───── Ruby
                    │
sysprobe-core ──────┼── ext-php-rs ─ PHP
                    │
                    └── C ABI
                         ├── Deno
                         ├── C/C++
                         ├── Perl
                         ├── Lua
                         └── other runtimes
```

The C ABI should remain deliberately small.

It should not dictate the internal Rust API.

---

# 28. C ABI Design

Do not expose Rust's ABI directly.

Rust's native ABI is not a stable cross-version FFI contract.

Instead expose:

```rust
extern "C"
```

functions using C-compatible types.

Complex Rust values such as:

```rust
Vec<T>
String
Option<T>
Result<T, E>
```

must not cross the C ABI directly.

Possible approaches include:

1. C-compatible structures.
2. Opaque handles with accessor functions.
3. Caller-provided buffers.
4. Serialized structured output.

For example:

```c
typedef struct {
    uint64_t bytes;
    bool limited;
} sysprobe_memory_limit;
```

or an opaque snapshot:

```c
sysprobe_snapshot *sysprobe_snapshot_current(void);

bool sysprobe_snapshot_memory_limit(
    const sysprobe_snapshot *,
    uint64_t *out_bytes
);

void sysprobe_snapshot_free(
    sysprobe_snapshot *
);
```

Opaque handles generally provide better ABI evolution than exposing large structures directly.

---

# 29. Snapshot Semantics

It may be useful to model observations as a snapshot.

For example:

```rust
let snapshot = sysprobe::snapshot()?;
```

A snapshot could contain mutually consistent observations gathered at approximately the same time.

This becomes relevant because values such as:

```text
memory.current
memory.max
CPU pressure
cgroup membership
```

can change while a program is reading them.

The library cannot make kernel state globally atomic, but explicit snapshot semantics make the temporal model clearer.

Alternatively, the API can expose live queries where appropriate.

This should be decided deliberately rather than accidentally.

---

# 30. Error Semantics

Resource introspection frequently encounters conditions that are not necessarily exceptional.

Examples:

```text
controller not enabled
resource unlimited
kernel feature unsupported
permission denied
file disappeared during query
process moved between cgroups
platform does not implement concept
```

The API should distinguish:

```text
unlimited
```

from:

```text
unknown
```

from:

```text
unsupported
```

from:

```text
failed to query
```

where those distinctions matter.

Using `Option<T>` for every case may lose too much information.

A richer representation may eventually be appropriate:

```rust
pub enum Availability<T> {
    Value(T),
    Unlimited,
    Unsupported,
    Unavailable,
}
```

Errors would remain reserved for actual failures where useful.

The precise model needs further design.

---

# 31. Dynamic Constraints

Constraints are not necessarily immutable.

A container orchestrator or service manager can change resource settings while a process is running.

Therefore:

```rust
let env = sysprobe::current()?;
```

should not necessarily imply that all values remain valid forever.

Potential API models include:

```rust
env.snapshot()
```

or repeated live queries:

```rust
env.memory().limit()?
```

The documentation should explicitly state whether returned information is:

```text
live
cached
snapshot-based
```

---

# 32. Security and Permissions

The library should assume that some operating-system information may be unavailable because of:

- filesystem permissions;
- sandboxing;
- namespace isolation;
- restricted `/proc`;
- Windows security descriptors;
- runtime sandbox policies.

Failure to retrieve optional metadata should not necessarily prevent retrieval of unrelated information.

For example, inability to identify a container runtime should not prevent reading a memory limit.

The implementation should degrade gracefully.

---

# 33. Recommended MVP

The first version should remain intentionally narrow.

A reasonable MVP is:

```text
Linux only
cgroup v2 read support
Rust API
Python binding through PyO3
```

Initial resources could include:

### CPU

```text
cpu.max
cpu.weight
cpuset.cpus.effective
```

### Memory

```text
memory.current
memory.max
memory.high
memory.swap.current
memory.swap.max
```

### Processes

```text
pids.current
pids.max
```

The MVP should already structure its API around portable semantic concepts rather than exposing only raw cgroup files.

That prevents the first implementation from permanently defining the abstraction as:

> a Rust cgroup parser.

---

# 34. Possible Development Sequence

A sensible progression is:

```text
Phase 1
Linux + cgroup v2 read
Rust + Python

Phase 2
Linux rlimits + CPU affinity
effective-limit/provenance model

Phase 3
Linux PSI + namespaces
broader environment introspection

Phase 4
cgroup v1 compatibility

Phase 5
C ABI
Deno or other lightweight bindings

Phase 6
Ruby binding

Phase 7
Windows Job Objects + CPU Sets

Phase 8
PHP and additional ecosystems
```

This order is intentionally flexible.

In particular, cgroup v1 support could move earlier if compatibility demand proves significant.

---

# 35. Naming

If the project remains strictly focused on Linux cgroups, a cgroup-specific name could be appropriate.

If the intended scope includes:

```text
Linux
Windows
limits
capacity
pressure
affinity
isolation
```

then a cgroup-specific name would unnecessarily constrain the project's identity.

The strongest general name considered so far is:

```text
sysprobe
```

It communicates:

```text
system inspection
probing
runtime discovery
low-level behavior
```

without tying the project to a particular kernel mechanism.

A possible description is:

> **sysprobe discovers the execution environment and effective resource constraints of a process across operating systems.**

Other names considered include:

```text
procenv
envprobe
resprobe
procscope
```

`procenv` is semantically strong because the process is the observation point, but it may evoke Linux `/proc` or environment variables.

`resprobe` communicates resources well but undersells isolation and other environment information.

`envprobe` is broad but potentially ambiguous.

`procscope` captures the notion of the scope within which a process operates, although its purpose is less immediately obvious.

`sysprobe` currently provides the best balance of breadth and recognizability.

---

# 36. Core Design Principles

The project should preserve the following principles as its scope grows.

### 1. Model semantics, not kernel files

Prefer:

```rust
memory.limit()
```

over making callers understand:

```text
/sys/fs/cgroup/.../memory.max
```

---

### 2. Keep raw platform information available

Normalization should not prevent advanced callers from inspecting the original mechanism.

---

### 3. Never confuse capacity with constraints

Physical RAM is not the same as usable RAM.

Host CPU count is not the same as available parallelism.

---

### 4. Never confuse CPU affinity with CPU quota

These constrain different dimensions of CPU execution.

---

### 5. Keep pressure separate from limits

Pressure describes contention, not policy.

---

### 6. Prefer actual constraints over container detection

Knowing that a process appears to run under Docker is less useful than knowing exactly which resource restrictions the kernel applies.

---

### 7. Do not force Linux concepts onto Windows

Expose common semantics where they genuinely exist.

Use platform-specific APIs where they do not.

---

### 8. Preserve provenance

Users should be able to understand why the library reports a particular effective constraint.

---

### 9. Keep the Rust core binding-agnostic

Python, Ruby, PHP, and Deno should be consumers of the model rather than influences on the core architecture.

---

### 10. Make bindings thin

Ideally:

```text
OS-specific discovery
        ↓
normalized Rust structures
        ↓
language conversion
```

rather than duplicating resource logic in every binding.

---

# 37. Strategic Positioning

The project should not primarily position itself as:

> A cross-platform cgroup library.

Nor merely as:

> A better CPU-count function.

A more durable abstraction is:

> **Process-centric system resource and execution-environment introspection.**

The process is the observation point.

The operating system may provide:

```text
Linux cgroups
Linux rlimits
Linux affinity
Linux namespaces
Linux PSI
Windows Job Objects
Windows CPU Sets
Windows affinity
```

The library translates those mechanisms into a coherent description of:

```text
What resources exist?
What resources can this process actually use?
What restrictions apply?
Where did those restrictions come from?
Which CPUs can execute it?
How much contention exists?
How is the process isolated?
```

That scope addresses the original Python/container problem while remaining useful independently of Python, containers, or cgroups.

---

# 38. Working Architectural Summary

The current conceptual architecture is:

```text
                         APPLICATIONS
                              │
          ┌───────────────────┼───────────────────┐
          │                   │                   │
       Python               Ruby                Rust
        PyO3               Magnus              native
          │                   │                   │
          └───────────────────┼───────────────────┘
                              │
                       NORMALIZED API
                              │
          ┌───────────────────┼────────────────────┐
          │                   │                    │
       Capacity          Constraints           Pressure
          │                   │                    │
          ├──────── Affinity / Isolation ─────────┤
          │                   │                    │
          └────────────── Provenance ─────────────┘
                              │
                       PLATFORM BACKENDS
                              │
             ┌────────────────┴────────────────┐
             │                                 │
           Linux                             Windows
             │                                 │
      ┌──────┼────────┐                ┌───────┼────────┐
      │      │        │                │       │        │
   cgroups rlimits affinity          Jobs   CPU Sets affinity
      │               │                │
     PSI          namespaces      system topology
```

Additional bindings can sit above either the native Rust API or a deliberately small C ABI:

```text
sysprobe-core
    │
    ├── PyO3 ───────────── Python
    ├── Magnus ─────────── Ruby
    ├── ext-php-rs ─────── PHP
    │
    └── C ABI
         ├── Deno
         ├── C
         ├── C++
         ├── Perl
         ├── Lua
         └── future runtimes
```

The difficult part of this project is therefore **not FFI**.

The central engineering problem is defining correct, portable semantics for process resource constraints while preserving the differences between the mechanisms that enforce them.
