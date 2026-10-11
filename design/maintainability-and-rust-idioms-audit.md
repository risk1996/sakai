# Maintainability and idiomatic Rust audit

Audit date: 2026-10-10. Follow-up scope updated: 2026-10-11.

This document retains unresolved follow-ups and ongoing design guidance.
Completed one-time findings have been removed. Publication metadata and
reconciliation of release/API documentation are deferred to the existing
release checklists.

## Overall assessment

Sakai's core architecture remains sound: portable parsers, owned Linux directory
handles, borrowed controller views, typed quantities, and explicit absent or
unlimited values. Preserve these choices as new interfaces arrive. A wholesale
rewrite, general parser framework, or aggregate snapshot abstraction is not
justified by the original audit.

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
unit parsing. Preserve separate validation of kernel behavior and installed
packages as the library grows.

## Remaining publication work (deferred)

Complete the remaining Cargo description, repository, and README metadata through
[the crates.io checklist](cratesio-prepublish-checklist.md). Reconcile package,
release, and design documentation with the implemented API through that checklist
and [the PyPI checklist](pypi-prepublish-checklist.md).

Apply publication checks to the actual release archives after packaged code or
metadata changes. Earlier verified artifacts do not establish the contents or
behavior of a new release candidate. Keep the declared Rust compatibility policy
and minimum-version CI checks aligned with dependency and language changes.

## Ongoing test strategy and uncertainty

Additional focused coverage would strengthen contracts that are central to
the architecture:

- A controlled cgroup rename verifies that a pinned handle continues to read
  while its diagnostic path becomes stale.
- Removal during traversal exercises the documented child-enumeration policy.
- Discovery races exercise process exit or migration between procfs reads.
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
useful evidence, but these checks alone do not establish those broader claims.

## Ongoing maintenance guidance

- Keep public enums extensible when kernel fields, controllers, or error categories
  can grow; keep genuinely closed alternatives exhaustive.
- Keep source counters integral and compute fractional rates through the supported
  typed helper. Handle counter resets or rollover before supplying a delta.
- Run doctests and warnings-denied documentation builds alongside ordinary checks.
  Validate Linux-only APIs on Linux; parser success alone does not validate live reads.
- Verify the installed Python extension against package exports and stubs whenever
  the native surface changes. Preserve optional counters, units, immutability,
  ownership retention, and structured exception information.
- Reuse the scalar-interface implementation for stable scalar patterns. Keep complex
  grouping and version-dependent semantics explicit. Do not add a general parser
  framework or optimize allocations without workload evidence.
- Treat public `uom` types and borrowed ordered maps as intentional API contracts.
  Preserve typed lookup, deterministic iteration, and unreported-field distinctions.
- Prefer explicit accessors and conversions for validated values. Choose control
  flow and local mutation for clear domain behavior and ownership.
- Keep shell limited to short external-tool orchestration; grow maintained workflows
  in `xtask`.

The original audit's external references remain useful for future changes:

- [Cargo SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html).
- [Rust API Guidelines: future proofing](https://rust-lang.github.io/api-guidelines/future-proofing.html).
- [Rust API Guidelines: predictability](https://rust-lang.github.io/api-guidelines/predictability.html).
- [Linux cgroup v2 documentation](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html).
