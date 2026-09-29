# Prepublish refactor 3: controller views and uniform CPU snapshots

## Goal and place in the sequence

Start after [`prepublish_refactor_1.md`](prepublish_refactor_1.md) and
[`prepublish_refactor_2.md`](prepublish_refactor_2.md) are complete and
reviewed. Replace the reader traits with borrowed controller views and return
the named snapshot type from every CPU interface reader. This is the
intentional public API break of the three-stage refactor.

The library remains read-only: each method opens and reads a fresh interface
file, so separate calls are not an atomic snapshot. Missing files still
return `Error::FileMissing`. Parsers and value types still compile on non-Linux
platforms; handles and views exist only on Linux. Follow the repository
`AGENTS.md`, `CODE_STYLE.md`, and `RTK.md` and preserve user changes.

## API to implement

[`v2/cpu/mod.rs`](../sakai-core/src/cgroup/v2/cpu/mod.rs) currently defines
`ReadCpu` on `Cgroup`; [`v2/core.rs`](../sakai-core/src/cgroup/v2/core.rs)
defines `ReadCore`. Replace them with cheap borrowed views, constructed by
inherent methods on the existing open `Cgroup`:

```rust,ignore
let cgroup = Cgroup::from_current_process()?;
let stat = cgroup.cpu().stat()?;
let quota = cgroup.cpu().max()?;
let burst: Time = cgroup.cpu().max_burst()?.value();
let controllers = cgroup.core().controllers()?;
```

Use public `Cpu<'a>` and `Core<'a>` view types under `v2::cpu` and `v2::core`,
each holding `&'a Cgroup`. A view must not clone, reopen, or cache the
directory. Preserve the current `Cgroup::{from_path, from_pid,
from_current_process, child, children, path}` API. Add future controllers as
separate views; do not create a generic controller registry for this stage.

The `Cpu` view methods and results are:

| Method | Result value |
| --- | --- |
| `stat()` | `CpuStat` |
| `stat_local()` | `CpuStatLocal` |
| `weight()` | `CpuWeight` |
| `weight_nice()` | `Nice` |
| `max()` | `CpuMax` |
| `max_burst()` | `CpuMaxBurst` |
| `pressure()` | `Pressure` |
| `uclamp_min()` | `CpuUclampMin` |
| `uclamp_max()` | `CpuUclampMax` |
| `idle()` | `CpuIdle` |

The last four differing methods are `max_burst`, `uclamp_min`, `uclamp_max`,
and `idle`: today the readers unwrap their public `FromStr` structs into
`Time`, `Ratio`, `MaxOr<Ratio>`, and `bool`. Remove those unwrapping maps so
live reads and portable parsing return the same named type. Existing
`.value()` methods provide the underlying quantity or boolean. Keep
`CpuWeight::Idle` and `CpuWeight::Shares(Weight)` and the validated `Nice`
type unchanged. `Core` should expose `kind() -> Result<CgroupType, Error>`
for `cgroup.type` (the root may lack it), plus `controllers()` and
`subtree_control()` returning `Vec<CgroupController>`. Use `kind` so callers
do not need the raw identifier `r#type`.

## Migration and documentation

1. Define the views and inherent `Cgroup::cpu()`/`core()` accessors under
   Linux cfg. Move existing trait implementation bodies into view methods,
   using the stage-2 `Cgroup::parse` boundary. Remove `ReadCpu` and
   `ReadCore` after all in-repository callers migrate; avoid retaining two
   competing public reader surfaces before the first stable release.
2. Update [`linux_live.rs`](../sakai-core/tests/linux_live.rs), handle tests,
   [`README.md`](../README.md), crate doctests, and examples. Assert named
   result structs for live single-value reads where practical, and use
   `.value()` only when the quantity itself is wanted. Keep optional-file
   behavior explicit and avoid treating unavailable controllers as failures
   in unprivileged smoke tests.
3. Search design and tool docs for old trait names and API examples. The
   earlier implementation brief has been replaced by
   [`project-direction.md`](project-direction.md), which does not prescribe
   reader traits. This is documentation alignment, not a change to packaging,
   manifests, release workflows, or publishing.

## Review gate

- `devenv test` passes on macOS and Linux. Public rustdoc shows controller
  views only on Linux and portable `FromStr` types on other platforms.
- Linux integration tests exercise every view method, including available
  root interfaces and the delegated VM fixture where applicable. Root
  `cgroup.type` and old-kernel optional files retain `FileMissing` behavior.
  The view introduces no second descriptor and reads remain fresh.
- No internal call sites import the old reader traits. README and doctests
  compile with `cgroup.cpu()`/`cgroup.core()`. The diff is limited to API
  migration, tests, and its documentation; do not add memory or other
  controller features here.
- If the VM fails while booting before tests execute, report that separately
  from the library test result. A Linux container or CI run should still
  compile and exercise the Linux-only code.
