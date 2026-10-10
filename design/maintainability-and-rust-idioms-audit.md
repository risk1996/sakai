# Maintainability and idiomatic Rust audit

Audit date: 2026-10-10.

Source baseline: `c103d2a2915245177c28a75428884b457ee8f5b7`
(`feat(core): memory.numa_stat, memory.zswap.*`), together with the working-tree
changes present during the audit. Those changes include Python bindings,
installed-package tests, Kubernetes fixtures, and CI/tooling updates. This is
an assessment of that checkout, not just the committed baseline.

Snapshot note: while the report was being drafted, the checkout advanced to
`7ff455ea6a75d3144092a6d81f6faeb3212b1121` and continued to receive working-tree
changes. The swap, zswap, and NUMA source files cited below were no longer
present, and `sakai-python/src/linux.rs` had decreased from 1,336 to 1,197
lines. This report preserves the original audit's findings and check results;
it does not claim a complete new audit of those subsequent changes. Relative
links navigate existing files, whose contents may differ from the audited
snapshot. Removed files are named as historical evidence and can be inspected
in the recorded source baseline.

## Overall assessment

Sakai has a sound core architecture and is broadly aligned with idiomatic
Rust. Its strongest choices are domain-specific types, owned Linux directory
descriptors, borrowed controller views, portable parsers, checked conversions,
and explicit handling of absent or unlimited values. These choices make it
possible to extend the read surface without redesigning the library.

The main long-term risks are public API commitments, incomplete portability
and documentation checks, loss of operational error context, and repetition
across controller implementations and Python declarations. These risks are
manageable while the project is preparing its first publication. They become
more expensive after consumers depend on the existing types and names.

The recommended direction is to preserve the architecture, tighten the public
contract, and consolidate a few stable implementation patterns. A wholesale
rewrite, a general parser framework, or an aggregate snapshot abstraction is
not justified by the evidence gathered here.

## Scope, method, and evidence

The review covered:

- Workspace and crate manifests, shared lints, and project coding guidance.
- `sakai` units, errors, parser primitives, CPU/memory/PSI snapshots,
  Linux discovery, directory traversal, and interface reads.
- `sakai-python` ownership, conversion, exception mapping, exports, stubs,
  and contract tests.
- Linux live tests, the `xtask` VM runner, pinned kernel fixtures, and the
  Kubernetes test configuration.
- Development tasks, CI, release checklists, and design documentation.

The audit combined source inspection, the configured checks, a separate
documentation build, and a compiled arithmetic probe. Relevant source and
reference paths were rechecked while drafting this report, revealing the
snapshot changes noted above. The recorded check results describe the
original audit run rather than an immutable release artifact or the newer
checkout.

No implementation, manifest, CI, or existing documentation changes were made
as part of the audit. This report is the resulting documentation artifact.

### Verification results

| Verification | Result | What it establishes |
| --- | --- | --- |
| `devenv --profile python test` on macOS | Passed; 43 tests, formatting, Clippy, doctests, Ruff, and Ty | The configured checks passed for the host-visible code and Python sources/stubs at the time of the run. |
| Core rustdoc build with warnings denied | Failed on an unresolved `Error::FileMissing` link in `memory.rs` | The portable documentation surface has a failure that the normal doctest task does not detect. |
| Compiled `Count / Time` probe | 100 events over one second produced zero | Integer event-rate arithmetic loses ordinary monitoring rates before a later unit conversion. |
| Linux handles and Python runtime tests | Source inspected; not executed locally | Their implementation and configured coverage were reviewed, but this audit does not establish their runtime success. |
| Kernel VM and Kubernetes suites | Configuration and tests inspected; not executed | The project has integration-test infrastructure; its results were not independently verified here. |

The 43-test result is specific to macOS. Linux-only modules and the native
Linux Python binding tests are conditionally excluded there. The Python
profile's Ruff and Ty tasks also do not install and exercise the extension.
The CI wheel/sdist jobs provide that separate runtime workflow on Linux.

The standard checks initially encountered sandbox restrictions on Nix cache
access and were rerun with elevated access, following the repository's tool
instructions. The successful results are from those reruns.

### Reading the findings

"Medium" denotes a demonstrated behavior or maintenance risk worth resolving
before API stabilization. "Low" denotes a bounded design or organization
improvement. These labels are prioritization judgments, not claims about
security severity or current deployment incidents.

"Reproduced" identifies an observed command result. "Observed" identifies a
property directly visible in the source. "Recommendation" identifies a design
judgment. Release omissions are assessed in the context of the project's
explicitly unfinished publication plan.

| ID | Priority | Finding | Evidence |
| --- | --- | --- | --- |
| F01 | Medium | Integer event rates erase common values | Reproduced |
| F02 | Medium | Extensible public enums are exhaustive | Observed |
| F03 | Medium | Documentation builds are outside the normal gate | Reproduced and observed |
| F04 | Medium | CI does not cover the claimed portable platform | Observed |
| F05 | Medium | Operational errors lose path or cause information | Observed |
| F06 | Medium | The Python contract has repeated declarations and incomplete parity | Observed |
| F07 | Low | Scalar-interface boilerplate will multiply with controllers | Observed; consolidation recommended |
| F08 | Low | Public APIs commit to dependency and collection representations | Observed; policy decision recommended |
| F09 | Low | Some conventions and `Deref` implementations reduce API clarity | Observed; idiom judgment |
| F10 | Medium before publication | Release metadata and design-document status remain unfinished | Observed; already partly tracked |

## Architectural strengths to preserve

### Portable parsing and Linux access have separate responsibilities

[The crate root](../sakai/src/lib.rs) and
[v2 module](../sakai/src/cgroup/v2/mod.rs) expose parsed values on every
platform while gating kernel handles on Linux. The parser layer does not
perform I/O. Controller readers connect a known interface filename to its
typed parser through one shared handle.

This separation lets parser behavior be tested without a Linux host and keeps
kernel-specific concerns concentrated. A new controller should follow this
pattern: portable values and parsers, then a small Linux reader view.

### Handle ownership matches the kernel interface

[`Cgroup`](../sakai/src/cgroup/v2/handle.rs) owns an `OwnedFd` for an open
directory. Reads use that descriptor rather than reopening the diagnostic
path. Constructors verify cgroup2 filesystem magic, and path handling rejects
parent traversal and symlinks. The `openat2` path has a component-wise fallback
for kernels that do not implement that syscall.

`Cpu`, `Memory`, and `Core` borrow the handle. Their lifetimes express ownership
directly, without requiring shared ownership in ordinary Rust callers. The
Python readers use `Arc<Cgroup>` because they must survive independently of
the Python object that created them. That is an appropriate adjustment at
the language boundary.

### Types capture useful distinctions

[`MaxOr<T>`](../sakai/src/limit.rs) distinguishes an unlimited value from
a concrete value. Optional counters distinguish a successful read with an
absent field from a counter containing zero. Validated `Weight`, `Nice`, and
`NonZeroTime` values constrain inputs. The unit system separates time, bytes,
pages, counts, and fractional ratios.

The microsecond-to-nanosecond parser uses checked multiplication, and
percentage parsing rejects non-finite or out-of-range values. The `dyn` marker
in `BaseUnits` is type-level bookkeeping, not runtime dynamic dispatch.

### Read semantics are explicit

The implementation and documentation consistently describe fresh reads and
the absence of a transaction across files. CPU quota ratios are explicitly
limited to the selected cgroup's quota and period; ancestor limits and
affinity are not silently folded into the result.

Preserve those semantics as new interfaces arrive. Writes and event delivery
should have their own contracts. Descriptor lifetime matters for interfaces
such as `memory.peak`: the current kernel documentation describes resets as
applying to subsequent reads through the same descriptor. That supports the
project's deliberate use of a fresh read-only descriptor for each call.
[Linux cgroup v2 documentation](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html).

### Existing tests target meaningful boundaries

The parser tests cover invalid values, conversion overflow, reordered fields,
unknown fields, and older-kernel forms. Discovery fixtures cover unified
hierarchies, bind mounts, namespace roots, deleted membership, and multiple
candidate mounts. The Python tests cover ownership retention, immutability,
non-UTF-8 paths, optional values, and exception attributes.

The kernel matrix and installed wheel/sdist jobs extend the checks beyond
unit parsing. Their existence is a strength even though those suites were
not run during this audit.

## Detailed findings

### F01 — Integer event rates erase common values

Priority: Medium. Evidence: Reproduced. Address before stabilizing the rate API.

[`unit.rs`](../sakai/src/unit.rs) defines `Count` and `Time` with `u64`
storage and defines `EventRate` as an integer frequency with nanoseconds as
the base time unit. Dividing these quantities performs integer division in
the base units.

The following probe was compiled against the local crate and executed:

```rust
use sakai::{Count, EventRate, Time};

fn main() {
  let count = Count {
    value: 100,
    ..Default::default()
  };
  let elapsed = Time {
    value: 1_000_000_000,
    ..Default::default()
  };
  let rate: EventRate = count / elapsed;
  println!("{}", rate.value);
}
```

The output was `0`. The mathematical rate is 100 events per second, or
`0.0000001` events per nanosecond. A later conversion to hertz cannot recover
the fraction that was discarded during division. More generally, rates
below one billion events per second truncate to zero with this calculation.

The behavior is documented and is consistent with the underlying integer
arithmetic. The concern is that it is an inconvenient default for a metrics
library. The current rate test uses two billion events over one second,
which proves dimensional compatibility but does not expose the ordinary-rate
case.

Recommendation: retain integral source counters and provide a convenient
fractional rate calculation. A typed floating-point result and an explicit
conversion/helper are possible approaches. The helper should define behavior
for zero elapsed time and document precision limits when converting large
integer counters. Counter reset or rollover handling should be explicit if
the API also computes deltas.

Acceptance criteria:

- Representative rates such as 100 events per second and one event over two
  seconds retain their values through the supported calculation path.
- Examples demonstrate that path using Sakai's public API.
- Zero duration and relevant numeric boundaries have defined behavior.
- Documentation makes the semantics of any retained integer rate type clear.

### F02 — Extensible public enums are exhaustive

Priority: Medium. Evidence: Observed. Address before the first public contract.

The known-controller variants in
[`CgroupController`](../sakai/src/cgroup/v2/core.rs), the field selectors in
[`memory/stat.rs`](../sakai/src/cgroup/v2/memory/stat.rs), and the variants
in [`error.rs`](../sakai/src/error.rs) are public and exhaustive. A consumer
can match all existing variants without a fallback arm. Adding a newly
recognized controller, metric, or error category can then break compilation.

`CgroupController::Other(String)` preserves unknown input names. It does not
make adding a named variant source-compatible with exhaustive downstream
matches. Cargo's SemVer guidance identifies new variants in an exhaustive
enum as a breaking change.
[Cargo SemVer guidance](https://doc.rust-lang.org/cargo/reference/semver.html#major-adding-new-enum-variants-without-non_exhaustive).

[`CpuStatField`](../sakai/src/cgroup/v2/cpu/stat.rs) and its
`bandwidth_fields`/`bandwidth_burst_fields` methods also expose parser schema
details publicly. Their public status expands the compatibility surface
without an evident consumer requirement.

Recommendation: classify each enum according to whether its domain is closed
or expected to grow. Mark growing categories `#[non_exhaustive]`, and keep
parser-only schema helpers private. `MaxOr<T>` has a naturally closed
two-state contract. Error-variant fields also deserve review if additional
context is likely to be added later.

Acceptance criteria:

- Each public enum has a deliberate extension policy.
- External consumer examples use fallback matches for extensible categories.
- Parser implementation helpers are public only when they serve a documented
  use case.
- Release notes account for compatibility if consumers already use the
  unpublished checkout.

### F03 — Documentation builds are outside the normal gate

Priority: Medium. Evidence: Reproduced and observed.

[`check:doc`](../devenv.nix) runs
`cargo test --workspace --all-features --doc --locked`. Doctests are useful,
but this task does not establish that the generated API documentation builds
without warnings.

The separate command used during the audit was:

```text
devenv shell -- rtk cargo rustdoc --package sakai-core --all-features --locked -- -D warnings
```

It failed on macOS with:

```text
error: unresolved link to `Error::FileMissing`
  --> sakai-core/src/cgroup/v2/memory.rs:183:47
```

`MemoryPeak` is portable, but the `Error` import used by its documentation link
is conditional on Linux. The standard checks passed while this failure
remained present.

Recommendation: qualify the link so it resolves on every documented target,
and add a warnings-denied rustdoc-build task alongside doctests. Once the
public surface is reviewed, consider a targeted missing-documentation lint
for library API items. Such a lint should support useful semantic
documentation rather than merely force boilerplate comments.

Acceptance criteria:

- Core documentation builds without warnings on Linux and macOS.
- Both doctests and documentation generation are covered by project tasks
  and the relevant CI jobs.
- A broken portable API link causes a check failure.

### F04 — CI does not cover the claimed portable platform

Priority: Medium. Evidence: Observed.

Every runner in [the current workflow](../.github/workflows/ci.yml) is
`ubuntu-26.04`. [The README](../README.md) says CI tests macOS and Linux.
The Linux kernel matrix varies kernel versions, but does not provide a
non-Linux compile, test, or documentation check.

Portable parser availability is part of the advertised contract. Conditional
imports and documentation can regress even when the Linux checks pass, as
F03 demonstrates. Local macOS success during this audit does not establish
continuous coverage.

Recommendation: add a macOS job for the Rust core and portable documentation,
while keeping the Python runtime and kernel suites on Linux. State separately
which operating systems support parsing and which support live reads. Broaden
the platform matrix further only when the support contract requires it.

Acceptance criteria:

- CI tests portable core behavior and documentation on macOS as well as Linux.
- README claims correspond to jobs that actually exist.
- Platform-gated API examples compile on their intended targets.
- Passing parser checks is not reported as validation of Linux runtime reads.

### F05 — Operational errors lose path or cause information

Priority: Medium. Evidence: Observed.

[`Error::read`](../sakai/src/error.rs) preserves the interface path for
missing files. For other I/O errors it returns `Io(io::Error)` and drops the
path. Mapping an unsupported operation to the fieldless `NotSupported` variant
drops both the original I/O error and the interface path.

[`Cgroup::from_pid`](../sakai/src/cgroup/v2/handle.rs) wraps procfs errors
with `io::Error::other`. This retains an error source but does not expose the
original OS errno directly through the outer `io::Error::raw_os_error`.
[Python exception conversion](../sakai-python/src/linux.rs) consults that
outer errno, so discovery failures can lose the information needed to select
a specific Python OS-error subtype.

`open_candidates` tries alternative matching mounts and returns only the
last failure. That fallback is useful, but it can also hide an earlier,
more informative candidate failure. This is a diagnostic limitation rather
than evidence that successful fallback is incorrect.

These distinctions matter when a caller polls many cgroups and must identify
which interface failed, or when permission and namespace failures are reported
from production systems.

Recommendation: preserve operation/path context alongside source errors and
carry native errno through discovery when it is available. Choose an explicit
policy for reporting failed mount candidates. The policy can remain compact;
retaining every attempted failure is not automatically necessary. Keep the
intentional missing-interface category distinct from generic I/O failure.

Acceptance criteria:

- Interface permission and unsupported-operation failures identify the
  relevant operation/path and preserve the useful source information.
- Procfs OS failures retain errno where the underlying error provides it.
- Python conversion preserves the documented category and diagnostic
  attributes, including errno/filename where appropriate.
- Failure tests check structured properties and source chains, not only
  formatted messages.

### F06 — The Python contract has repeated declarations and incomplete parity

Priority: Medium. Evidence: Observed.

The public surface is maintained independently in:

- [`linux.rs`](../sakai-python/src/linux.rs): native classes, methods,
  conversions, documentation, and module registration.
- [`__init__.py`](../sakai-python/python/sakai/__init__.py): imports and
  `__all__`.
- [`_sakai.pyi`](../sakai-python/python/sakai/_sakai.pyi): public type stubs.
- [`test_contract.py`](../sakai-python/tests/test_contract.py): a manually
  listed inventory of documented public members.
- [`pyo3-bindings.md`](pyo3-bindings.md): the planned public mapping.

This repetition creates opportunities for a new method to be exported but
mistyped, omitted from the package, or absent from the test inventory. Ty
checks the Python sources and stubs; it does not prove the installed extension
matches those declarations.

Rust exposes memory `events`, `events_local`, and `oom_group`. The current
Python `MemoryReader` does not expose them, although the binding plan includes
them. That is an incomplete planned surface, not proof that an advertised
existing Python method is broken.

There is also a naming issue: Python `CpuWeight.shares` contains the v2
`cpu.weight` value. The Kubernetes test explicitly acknowledges this. A name
that callers associate with v1 shares makes resource conversions harder to
interpret.

The Linux bindings file is 1,336 lines at review time and contains exceptions,
handle management, all controller readers, snapshot conversions, mapping
construction, registration, and tests. File length alone is not a defect, but
these responsibilities are independently understandable and will grow.

Recommendation: document the supported subset or complete the planned parity;
settle weight terminology; split the bindings by responsibility; and automate
contract consistency checks. Begin with a check that compares native exports
and members with the stubs and package exports. Generate repetitive
declarations only if doing so remains simpler than maintaining them manually.

Acceptance criteria:

- The supported Rust-to-Python mapping is explicit and agrees with the API.
- Native exports, Python exports, and stub members are checked for consistency.
- New methods enter the installed-package tests through a repeatable workflow.
- Weight naming and documentation distinguish v2 weight from v1 shares.
- Module organization lets a controller change be reviewed without navigating
  unrelated ownership and exception code.

### F07 — Scalar-interface boilerplate will multiply with controllers

Priority: Low. Evidence: Observed. Recommendation: consolidate stable repetition.

At the audited snapshot, [`memory.rs`](../sakai/src/cgroup/v2/memory.rs),
`sakai/src/cgroup/v2/memory/swap.rs`,
`sakai/src/cgroup/v2/memory/zswap.rs`, and the simple CPU parsers repeatedly
defined a private value field, derived traits, a
`value()` accessor, a small `FromStr` implementation, and similar boundary
tests. The domain distinctions are valuable, but much of their implementation
is mechanical.

The shared `Parser`, `ParseCgroup`, and `KeyedFields` primitives already remove
important repetition. The next opportunity is a small private scalar-interface
macro with explicit type, encoding, field name, and documentation. Controller
methods can stay ordinary Rust functions so filenames and return types remain
easy to inspect.

The CPU and event keyed parsers also collect intermediate vectors before
building maps. That is an allocation opportunity, but the audit found no
workload evidence establishing it as a performance problem. A fixed builder
may eventually suit a fixed schema. That should be a separate decision from
reducing repetitive source code.

Recommendation: consolidate the stable scalar pattern while retaining domain
types and interface-specific documentation. Keep complex grouping logic and
version-dependent semantics explicit. Continue with eager handwritten parsers
for the existing small text formats. The existing
[parser design research](parser.md) is broadly consistent with this direction.

Acceptance criteria:

- A new scalar interface requires a concise declaration and its distinct
  semantics, rather than copied parsing/accessor boilerplate.
- Macro failures remain understandable and generated types retain useful docs.
- Shared parser boundary tests are complemented by meaningful domain tests.
- Performance changes are supported by an identified workload or measurement.

### F08 — Public APIs commit to dependency and collection representations

Priority: Low. Evidence: Observed. Recommendation: make the commitment deliberate.

The public aliases in [`unit.rs`](../sakai/src/unit.rs) expose `uom`
quantities directly. `Count` and `Pages` examples construct the quantity's
public `value` field. `BaseUnits`, `CountKind`, and `PageKind` are also public.
Consumers can consequently depend on the unit library's exact type and trait
relationships.

The statistics accessors in
[`memory/stat.rs`](../sakai/src/cgroup/v2/memory/stat.rs) and the audited
`sakai/src/cgroup/v2/memory/numa_stat.rs` expose references to concrete
`BTreeMap` types. This provides convenient, deterministic
iteration, but makes replacing that storage with another representation an
API change.

These are valid API designs. The concern is accidental commitment, not the
mere presence of dependencies or maps. Newtypes and narrower accessors can
hide implementation choices, as discussed in the
[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/future-proofing.html#newtypes-encapsulate-implementation-details-c-newtype-hide).

Recommendation: decide whether `uom` interoperability and direct map access
are promises Sakai intends to maintain. If so, document them and centralize
duplicated `uom` dependency settings in workspace dependencies. If storage
freedom is important, consider typed lookup and iterator APIs before release.
Retain useful dimensional typing in either case; this review does not require
replacing it with primitives or rejecting the project's `uom` preference.

Acceptance criteria:

- Public dependency and representation commitments are intentional.
- Consumer examples show the supported construction and conversion APIs.
- Shared dependency settings cannot drift between core and Python crates.
- Any narrower API preserves field availability distinctions and units.

### F09 — Some conventions and `Deref` implementations reduce API clarity

Priority: Low. Evidence: Observed. Recommendation: refine conventions.

[`CODE_STYLE.md`](../CODE_STYLE.md) appropriately emphasizes domain semantics,
validation, checked values, consistency, and concise tests. Its blanket
preferences for `match` over `if`, avoiding mutation, and associated functions
are more prescriptive than idiomatic Rust requires.

A boolean `match` can be more verbose than a branch. A locally mutable parser
builder can express ownership and validation clearly. A module-level helper
can organize an operation without introducing a stateless namespace struct.
The `PythonError` and `PythonPath` helper structs are examples of the latter
organizational pattern in the bindings; they are not correctness defects.

Validated `Weight`, `Nice`, and `NonZeroTime` derive `Deref`. This permits
implicit access to the wrapped type's methods and couples method resolution
to that type. The
[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/predictability.html#only-smart-pointers-implement-deref-and-derefmut-c-deref)
recommend reserving `Deref` for smart-pointer behavior. Explicit accessors,
`AsRef`, and consuming conversions provide clearer boundaries for validated
value objects. The current code does not derive `DerefMut`, so this finding
does not claim that callers can mutate around validation through dereferencing.

The shell guidance also says committed Bash should be avoided, while the
Kubernetes task in `devenv.nix` contains a multi-step shell workflow. It is
reasonable for a short task to invoke external tools in shell, but the written
policy should state the intended exception or the workflow should move into
the existing Rust `xtask` as it grows.

Recommendation: keep immutability as a preference, allow ordinary Rust control
flow and helpers when they clarify intent, and review `Deref` on domain values.
Clarify the boundary between short tool orchestration and maintained scripts.
This report does not amend or override the current coding rules.

Acceptance criteria:

- Coding guidance explains when exceptions improve clarity.
- Validated types expose an intentional conversion and accessor contract.
- Routine changes are reviewed for understandable domain behavior rather than
  adherence to a single expression style.
- Shell orchestration follows an explicit repository policy.

### F10 — Release metadata and design-document status remain unfinished

Priority: Medium before publication. Evidence: Observed; partly already tracked.

[`sakai/Cargo.toml`](../sakai/Cargo.toml) has no declared
`rust-version`, license, description, repository, or README metadata. The
project already identifies publication preparation in
[project-direction.md](project-direction.md) and the
[crates.io checklist](cratesio-prepublish-checklist.md). These omissions are
expected unfinished release work, not evidence of a failed release process.

A locked development toolchain does not define the compatibility promise to
downstream Rust consumers. Choose an MSRV policy, set the manifest field, and
verify the selected floor with the dependencies and language features that
the crate actually uses. A numeric minimum should follow that verification.

Design-document status has also drifted:

- [The binding plan](pyo3-bindings.md) says no binding implementation exists.
- [The PyPI checklist](pypi-prepublish-checklist.md) says the repository has no
  Python extension or release artifacts.
- The implementation and CI now include a binding crate and workflows that
  build and test distributions.

The last point establishes the presence of implementation and build workflows;
this audit did not inspect an actual release archive. The documents should
distinguish historical plans, implemented behavior, and outstanding release
gates so new contributors can determine which statements are current.

Recommendation: complete metadata, licensing, and MSRV work through the
existing release checklists. Update document status and add a brief supported
API/kernel-availability policy. Only claim kernel-version boundaries that
have been established from sources or tests; optional-file behavior already
provides a useful compatibility foundation.

Acceptance criteria:

- The Rust compatibility policy is declared and checked in CI.
- Package metadata and license files are present in the reviewed archives.
- Historical plans are labeled, and current API documents agree with source.
- The publication checklist is applied to the packaged artifacts, not only
  the working tree.

## Test strategy and remaining uncertainty

The immediate testing improvements should target demonstrated gaps: fractional
rates, portable documentation, and binding/stub agreement. They should not
duplicate every wrapper's implementation or expand into an unrelated test
framework migration.

Additional focused coverage would strengthen contracts that are central to
the architecture:

- A controlled cgroup rename verifies that a pinned handle continues to read
  while its diagnostic path becomes stale.
- Removal during traversal exercises the documented child-enumeration policy.
- Discovery failures exercise process disappearance and errno preservation.
- Older-kernel tests exercise optional interfaces and counters through the
  language boundary where that compatibility is promised.

These are coverage recommendations, not reproduced lifecycle bugs. Tests
that configure cgroups should use the existing disposable Linux fixture or
VM environment. The ordinary host-read suite should keep its unprivileged
behavior.

Keep unknown-key tolerance and duplicate-key policy distinct. Unknown keys
are intentionally ignored where their semantics are unavailable. Duplicate
handling is explicitly deferred in the project TODO, and the memory parser
already tests last-value behavior. This report does not treat that documented
scope decision as an undiscovered correctness defect. When the policy is
stabilized, make its application across parsers explicit.

The audit does not establish parser throughput, allocation cost under large
polling workloads, support across every kernel configuration or architecture,
free-threaded Python compatibility, dependency advisory status, or the
contents of final publication artifacts. The source and existing tests give
useful evidence, but none of those broader claims follows from the local
43-test result.

## Recommended sequence

### Before freezing the first public API

1. Resolve F01 with a usable fractional-rate calculation and representative
   examples/tests.
2. Review public enum extensibility and parser helper visibility in F02.
3. Settle operational error context and Python exception information in F05.
4. Decide the Python subset, terminology, and representation commitments in
   F06/F08; review domain-value `Deref` before those types become established.
5. Define the Rust compatibility policy and complete the publication metadata
   work in F10.

These items affect consumer contracts. Finishing them while publication is
still pending reduces later compatibility work.

### Verification and documentation improvements

1. Fix the demonstrated rustdoc failure and add documentation generation to
   the normal gate.
2. Add macOS portable-core coverage and correct the CI support statement.
3. Add native/stub/export consistency checks and update design-document status.
4. Use the existing installed-package and VM workflows to validate the
   resulting release candidate on Linux.

### As the next controllers are added

1. Introduce a small scalar-interface abstraction if it reduces the repeated
   declarations identified in F07.
2. Organize Python readers and conversions by responsibility.
3. Add focused lifecycle/error tests for the contracts described above.
4. Measure complete read-path performance if polling scale becomes a concrete
   requirement, then select optimizations from the measured costs.

This sequence preserves the current strengths while making future interface
additions easier to implement, verify, and evolve.

## Reproduction and source references

The recorded project checks were invoked with:

```text
devenv --profile python test
```

The independent documentation check was invoked with:

```text
devenv shell -- rtk cargo rustdoc --package sakai-core --all-features --locked -- -D warnings
```

The rate probe in F01 was compiled with the project Rust toolchain against a
local `sakai-core` library artifact and executed in a temporary directory.
Its result is recorded above; the temporary executable was removed. Rebuild
against the source revision under review when repeating the probe rather
than depending on a cached artifact filename.

Repository evidence is linked beside each finding. External primary sources
used to assess compatibility and Rust API conventions are:

- [Cargo SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)
  for public enum evolution.
- [Rust API Guidelines: future proofing](https://rust-lang.github.io/api-guidelines/future-proofing.html)
  for representation commitments and newtype boundaries.
- [Rust API Guidelines: predictability](https://rust-lang.github.io/api-guidelines/predictability.html)
  for `Deref` and method-resolution expectations.
- [Linux cgroup v2 documentation](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html)
  for the kernel interface and descriptor-specific peak semantics.

The report records findings and proposed follow-up work. It does not adopt
new architecture decisions or replace the project's release checklists.

To inspect a removed file from the original source baseline, use its historical
path, for example:

```text
rtk git show c103d2a2915245177c28a75428884b457ee8f5b7:sakai-core/src/cgroup/v2/memory/swap.rs
```
