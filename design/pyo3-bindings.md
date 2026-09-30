# PyO3 binding research and implementation plan

Research checked on 2026-09-30. This is a plan for bindings to the **current**
read-only `sakai-core` cgroup v2 API, not the broader cross-platform resource
model explored in [bindings.md](bindings.md). No binding implementation exists
yet. The Rust core remains the source of truth for discovery, parsing, units,
and kernel semantics.

## Recommendation

Add a `sakai-python` workspace member containing a PyO3 extension, and package
it as the Python import `sakai` with Maturin. Ship an initial Linux CPython
package exposing the implemented `Cgroup`, CPU, memory, and core readers.
Return immutable, named Python snapshot objects for compound readings and
ordinary Python scalar values for single-value readings. Keep each method a
fresh read, matching Rust. Do not introduce a cached aggregate snapshot or
claim that several calls are atomic.

Use PyO3 0.29.x and Maturin 1.x as the implementation baseline, pinning exact
versions in `Cargo.lock` and the Python build lock during implementation.
PyO3's current guide is at 0.29.2; the exact patch should be selected when the
crate is added. Set Python 3.10 as the minimum supported version in package
metadata (`requires-python = ">=3.10"`) and target `abi3-py310` for the first
release. Build with a Python 3.10-or-newer interpreter. This reduces normal
CPython wheel variants, but these `abi3` wheels do **not**
load in free-threaded CPython. Support for free-threaded Python warrants a
separate build/test matrix and wheel decision after the first binding is working.
The extension should still use thread-safe Rust state and avoid relying on the
GIL for correctness.

Linux is the only runtime target for this first package because the handle API
is `#[cfg(target_os = "linux")]`. `sakai-core` parsers remain portable; do not
publish macOS/Windows wheels that can import but cannot perform the advertised
queries. Revisit portable parser bindings if there is a concrete use case.

## Repository facts that shape the binding

| Current source | Consequence for Python |
| --- | --- |
| [`Cgroup`](../sakai-core/src/cgroup/v2/handle.rs) owns an open directory; `cpu()`, `memory()`, and `core()` borrow it. | Python reader objects must keep the owner alive, for example with `Arc<Cgroup>` in their native wrappers. A Python `Cgroup` is not reconstructed from its display path on each call. |
| `from_current_process`, `from_pid(u32)`, `from_path`, `child`, and `children` are implemented. | Expose constructors and traversal; accept `os.PathLike` paths and child names without lossy UTF-8 conversion. Validate PID before conversion, including negatives, zero, and values above `i32::MAX`. |
| `Cgroup::path()` is diagnostic and can become stale. | Expose a `path` property for display only; never use it to perform subsequent reads. |
| Each reader opens and reads an interface anew. | Methods stay methods, not cached properties. Document that values from separate methods are not an atomic snapshot. |
| [`Error`](../sakai-core/src/error.rs) distinguishes missing interfaces, unsupported operations, non-v2 filesystems, deleted cgroups, parse failures, and I/O. | Preserve these categories in Python exceptions, including the interface path and underlying OS error where available. |
| [`MaxOr<T>`](../sakai-core/src/limit.rs) distinguishes `max` from a numeric value. | Map `Max` to `None` only inside a successful limit reading. A missing file still raises an exception. |
| [`Time`, `Bytes`, `Count`, `Pages`, `Ratio`](../sakai-core/src/unit.rs) are `uom` quantities. | Return Python `int` nanoseconds/bytes/count/pages and `float` dimensionless ratios; encode units in attribute names and stubs. Do not pass `uom` types through FFI. |
| [`TODO.md`](../TODO.md) has unfinished swap, pids, I/O, and cpuset work. | Bind only implemented readers; add later surfaces as their core readers land. The old conceptual MVP in `bindings.md` lists some interfaces that are still unimplemented. |

## Proposed public Python contract

The package should offer `sakai.Cgroup.current()`,
`sakai.Cgroup.from_pid(pid)`, and `sakai.Cgroup.from_path(path)`. A cgroup has
`path`, `child(name)`, `children()`, `cpu()`, `memory()`, and `core()`.
`children()` returns a list of separate pinned handles. The three view methods
return lightweight reader objects holding the same owner. All objects are
read-only at the Python level.

The reader methods mirror existing Rust names. The return mapping is:

| Rust read surface | Python return |
| --- | --- |
| `cpu.stat()`, `stat_local()` | Immutable `CpuStat` with `time` and optional `bandwidth`; immutable nested `CpuTimeStat`, `CpuBandwidthStat`, `CpuBurstStat`, and `CpuStatLocal` values. Time fields end in `_ns`; counts are integers. Preserve the difference between absent bandwidth and zero counters. |
| `cpu.max()` | Immutable `CpuMax(quota_ns: int | None, period_ns: int)` with `cpu_count: float | None`, documented as this cgroup's quota/period only. |
| `cpu.weight()` | Immutable `CpuWeight(is_idle: bool, shares: int | None)`. Idle is distinct from a zero share; shares are 1–10,000. |
| `cpu.weight_nice()`, `max_burst()`, `idle()` | `int` nice value, `int` nanoseconds, and `bool`, respectively. |
| `cpu.uclamp_min()`, `uclamp_max()` | `float` ratio and `float | None` ratio. `None` means the kernel's `max`, not zero. |
| `cpu.pressure()`, `memory.pressure()` | Immutable `Pressure(some, full)` and `PressureLine(avg10, avg60, avg300, total_ns)`. Ratios are 0–1; `full` can be `None` on older CPU PSI. |
| `memory.current()`, `peak()`, `low()`, `min()` | `int` bytes. |
| `memory.max()`, `high()` | `int | None` bytes; `None` means `max`. |
| `memory.oom_group()` | `bool`. |
| `memory.events()`, `events_local()` | Immutable `MemoryEventCounts` values (or two distinct wrappers around the same fields), including `None` for version-dependent optional counters. Keep hierarchical and local methods distinct. |
| `memory.stat()` | Immutable `MemoryStat` with separate read-only `bytes`, `pages`, and `counts` mappings. Keys use the kernel's snake_case field names; unknown fields remain ignored, matching the parser. Do not collapse page counts into byte amounts. |
| `core.kind()`, `controllers()`, `subtree_control()` | Topology as a documented string literal (`domain`, `domain threaded`, `domain invalid`, `threaded`); controller names as `list[str]`, retaining `Other(String)` names. |

Use frozen PyO3 classes for compound values, with explicit conversion
functions from the corresponding Rust types. Alternatively, a small Python
dataclass layer is acceptable if it preserves the same frozen, typed contract;
choose one representation consistently before implementation. Native wrappers
avoid importing Python classes from Rust and keep conversion local. Add `__repr__`
and equality only where useful for debugging and tests. Never expose mutable
Rust state or `#[pyclass]` wrappers around borrowed `Cpu<'_>`, `Memory<'_>`, or
`Core<'_>` directly.

Example target usage:

```python
from sakai import Cgroup, InterfaceMissingError

cgroup = Cgroup.current()
stat = cgroup.cpu().stat()
quota = cgroup.cpu().max()
print(stat.time.usage_ns, quota.cpu_count)

try:
    current_bytes = cgroup.memory().current()
except InterfaceMissingError:
    current_bytes = None  # memory controller/root interface unavailable
```

`None` from `quota.cpu_count` means unlimited **at this cgroup**. It does not
mean unlimited effective process CPU capacity: ancestor quotas, affinity, and
other policy are outside this API.

## Error and path contract

Create a small Python exception hierarchy: `SakaiError` as the common base,
with `InterfaceMissingError`, `NotCgroupV2Error`, `DeletedCgroupError`, and
`CgroupParseError`. Give `InterfaceMissingError` a `.path` attribute; callers
can distinguish a missing optional kernel interface from a missing cgroup
directory. Map `NotSupported` to a dedicated `SakaiError` subclass. For
`Error::Io`, retain the native `OSError` category, `errno`, and message; wrapping
it in `SakaiError` would lose useful standard exception behavior. For parse
failures expose the interface path and a concise reason; retain the original
Rust error text as context, and avoid placing entire large raw interface files
in the default exception message if it is unwieldy. Tests should assert types,
paths, and `errno`, not only string renderings.

The `abi3-py310` target cannot subclass native Python exception types through
PyO3's limited API (that capability starts at Python 3.12). Keep the custom
hierarchy rooted in `SakaiError` and preserve ordinary `OSError` separately.

PyO3 has `PathBuf` extraction through `os.fspath()` and converts OS strings
without requiring UTF-8. Use this for `from_path`, `child`, and the path
property. Test a non-UTF-8 child name on Linux and reject slash/traversal via
the core's existing validation. Python integer conversion should reject a PID
outside the core's valid positive `i32` range as `ValueError` or `OverflowError`
before discovery. Keep `Cgroup::from_pid` as the final validator.

## Build layout and development workflow

Proposed files:

```text
Cargo.toml                         # add sakai-python workspace member
sakai-python/
  Cargo.toml                       # lib.name = "_sakai", crate-type = ["cdylib"]
  pyproject.toml                   # Maturin build backend, Python metadata
  src/lib.rs                       # #[pymodule] _sakai and conversion modules
  python/sakai/__init__.py         # re-export supported public names
  python/sakai/_sakai.pyi          # extension API stubs
  python/sakai/py.typed
  tests/                           # Python contract tests
```

Set `[tool.maturin] python-source = "python"` and
`module-name = "sakai._sakai"`; the `#[pymodule]` initialization name must be
`_sakai`. Keep the PyO3 dependency out of `sakai-core`. Use Maturin >=1.9.4,
which sets `PYO3_BUILD_EXTENSION_MODULE` when building the extension; current
PyO3 guidance deprecates the `extension-module` Cargo feature because it
interferes with Rust tests. Check the packaged source distribution in isolation:
the extension crate depends on `../sakai-core` and the workspace root, so the
sdist must contain enough workspace files to rebuild without the checkout.

The repository already has `profiles.python` in `devenv.nix` with Python, uv,
Maturin, Ruff, and Ty, but it has no `pyproject.toml` or Python checks. Add a
binding-specific check to `devenv test` only after the extension exists. Use
the existing command conventions (`devenv shell -- rtk ...` for project tools;
`devenv test` or `devenv tasks run check:*` for checks). Pin and test the Python
interpreter used by Maturin instead of silently building against whichever
interpreter happens to be on `PATH`. Run the Python contract tests on 3.10 and
each newer minor version supported by the release; keep `.pyi` syntax and
runtime annotations valid on 3.10.

## Implementation sequence

1. **Scaffold packaging.** Add the workspace crate, Maturin config, Python
   package shell, stubs, and a minimal import smoke test. Make a local wheel,
   install it into a clean Python 3.10 virtual environment, and confirm
   `import sakai`.
2. **Bind the pinned handle and errors.** Implement constructors, ownership of
   reader views, path and child traversal, and one common Rust-error translator.
   Verify missing interface, non-v2 path, invalid PID, deleted-path, and
   permission-error behavior where reproducible.
3. **Bind CPU.** Cover every method currently implemented in
   `sakai-core/src/cgroup/v2/cpu/mod.rs`, including older-kernel optional
   fields. Share conversion helpers for `Time`, `Count`, `Ratio`, and `MaxOr`.
4. **Bind memory and core.** Cover all implemented memory and core methods;
   preserve local versus hierarchical events and byte/page/count categories.
5. **Complete the Python contract.** Fill in `.pyi` and `py.typed`, docstrings,
   usage examples, and lint/type checks. Run wheel and sdist install tests
   from outside the checkout to catch accidental source-tree imports.
6. **Integrate CI and release preparation.** Run Rust checks and Python tests on
   Linux, exercise old-kernel behavior with existing VM fixtures where useful,
   build audited Linux wheels for supported architectures, and test a fresh
   install of each wheel on Python 3.10 and each supported newer minor version.
   Confirm the wheel carries a `cp310-abi3` tag. Add publication only after
   project license/metadata and package-name ownership are settled;
   `project-direction.md` already tracks Rust crate release metadata as
   unfinished.

Acceptance criteria: every currently implemented CPU, memory, and core reader
is reachable from Python; all unit/limit/optional distinctions above have
conversion tests; Python 3.10 imports and passes the contract tests; live
Linux reads work without privilege; a missing optional
kernel file raises the documented exception; multiple calls are documented as
non-atomic; an installed wheel and a separately rebuilt sdist both import and
pass the package tests. No method writes to cgroupfs.

## Research notes and primary sources

- [PyO3 0.29 guide](https://pyo3.rs/main/) identifies the current API baseline.
  [Its module guide](https://pyo3.rs/main/module) requires the initialization
  name to match the extension file name and documents `#[pymodule]` exports.
- [PyO3 classes](https://pyo3.rs/main/class) explains native `#[pyclass]`
  values and their thread-safety constraints. This is why the plan uses owned
  reader state instead of Rust borrowed views in Python objects.
- [PyO3 building and distribution](https://pyo3.rs/main/building-and-distribution)
  documents `cdylib`, Maturin's build environment, and the deprecated
  `extension-module` feature. It also documents `abi3-py310` and the
  Python 3.12 threshold for subclassing native exception types through the
  limited API. [Its feature guide](https://pyo3.rs/main/features)
  distinguishes `abi3` from `abi3t`; an `abi3` wheel does not cover
  free-threaded CPython.
- [PyO3 parallelism](https://pyo3.rs/main/parallelism) recommends
  `Python::detach` for Rust-only work. Apply it around potentially blocking
  discovery and file reads once inputs and the native handle are owned; convert
  Python return objects after reattaching. Benchmark before treating the extra
  transition as a throughput optimization for tiny cached reads.
- [PyO3 free-threading](https://pyo3.rs/main/free-threading.html) says recent
  PyO3 modules default to declaring thread-safety. Audit the native wrappers
  and run a free-threaded build test before distributing such wheels.
- [PyO3 path conversion](https://pyo3.rs/main/doc/src/pyo3/conversions/std/path.rs)
  uses `os.fspath()` and includes a non-UTF-8 round-trip test.
- [PyO3 exceptions](https://pyo3.rs/main/exception.html) documents custom
  exception types and conversion to `PyErr`.
- [Maturin project layout](https://www.maturin.rs/project_layout) recommends a
  mixed Python/Rust package with `python-source`, dotted `module-name`, type
  stubs, and `py.typed`. [Maturin local development](https://www.maturin.rs/local_development)
  documents `maturin develop` and editable installs.
- [Python Packaging User Guide on binary extensions](https://packaging.python.org/en/latest/guides/packaging-binary-extensions/)
  explains stable-ABI and platform-wheel tradeoffs. Its
  [wheel format guide](https://packaging.python.org/en/latest/discussions/package-formats/)
  explains why wheel tags must match the shipped interpreter and platform.

These sources establish mechanics, not Sakai's public naming or error policy;
the latter are proposed decisions above and should be tested against the first
working binding before freezing the Python API.
