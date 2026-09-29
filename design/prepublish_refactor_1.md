# Prepublish refactor 1: shared discovery and keyed fields

## Goal and place in the sequence

This is the first of three independent review stages. Consolidate duplicated
Linux cgroup discovery and keyed-field access while preserving current
successful results and parsing behavior. Stage 2 moves shared types and improves error
provenance; stage 3 changes the reader API. Finish and review this stage before
starting either later stage.

The library is read-only. Parsers must still compile on non-Linux platforms;
directory handles and procfs discovery remain Linux-only. Follow the repository
`AGENTS.md`, `CODE_STYLE.md`, and `RTK.md`. Existing staged or uncommitted work
belongs to the user and must be preserved.

## Current implementation and required behavior

- [`v2/handle.rs`](../sakai-core/src/cgroup/v2/handle.rs) implements
  `Cgroup::from_pid`, `from_current_process`, and descriptor verification.
  `from_pid` reads the target process's `/proc/<pid>/cgroup` but resolves it
  against the **caller's** mount namespace. It considers matching cgroup2
  mounts deepest-root-first and tries another candidate if opening one fails.
  A ` (deleted)` membership is reported as `Error::DeletedCgroup`.
- [`v2/path.rs`](../sakai-core/src/cgroup/v2/path.rs) independently discovers
  the current process's path and mount metadata. Its `CgroupPath` reports the
  mount point and read-only flag, which the VM test fixture uses. It currently
  selects one deepest matching mount and does not check the deleted suffix.
- [`v2/fixtures`](../sakai-core/src/cgroup/v2/fixtures) includes unified and
  bind-mounted procfs examples. Preserve bind-mount and namespace-root
  behavior. Do not replace descriptor-based reads with reads by display path.
- [`cpu/stat.rs`](../sakai-core/src/cgroup/v2/cpu/stat.rs) parses `key value`
  lines into a map and supplies required typed fields. The CPU stat parser
  ignores unknown counters. [`common/pressure.rs`](../sakai-core/src/cgroup/common/pressure.rs)
  parses `some`/`full` lines containing `key=value` tokens; it ignores unknown
  kinds and fields and requires `some`. These formats have different lexical
  rules but repeat map storage and required-field conversion.
- Existing maps overwrite earlier values when a key occurs twice. **Do not add
  duplicate-key detection or change this behavior.** The request explicitly
  excludes duplicate detection.

## Implementation

1. Add a Linux-only private `cgroup/v2/discovery.rs`. Put unified-membership
   selection, deleted-membership handling, and matching-mount candidate
   construction/order there. Represent a candidate with its resolved path,
   mount point, read-only flag, and root depth as needed. Make the candidate
   computation testable separately from opening an actual cgroup directory.
   Keep `Cgroup::from_pid` responsible for opening candidates and verifying
   cgroup2 filesystem magic; keep `CgroupPath` as the public mount/path view.
2. Make both public discovery paths consume the same candidate logic. For a
   target PID, use that PID's membership and the caller's mountinfo. For
   `CgroupPath::current`, both inputs belong to the current process. Preserve
   the existing public error variants; add
   `CgroupPathError::DeletedCgroup { path: PathBuf }` for the previously
   unreported deleted case. Map a private discovery failure into each API's
   error type. A deleted membership must not produce a live `CgroupPath`.
3. Add a small crate-private keyed-field container near
   [`common/parser.rs`](../sakai-core/src/cgroup/common/parser.rs). It should
   retain the complete raw file for errors, hold borrowed key/value tokens,
   and provide required, optional, and presence lookups using the existing
   `ParseCgroup<Unit>` conversions. Let each caller tokenize its own format:
   whitespace-separated lines for `cpu.stat`, assignment tokens after the PSI
   kind for pressure. Keep unknown-field policy at those callers. Do not build
   a schema framework or add a dependency.
4. Replace the repeated storage/lookups in `CpuStatFields` and
   `PressureFields` with the shared container. Preserve public `FromStr`
   results, complete-file error context, missing/invalid-field behavior,
   optional CPU bandwidth groups, and older CPU PSI files with no `full` line.
   Keep field enums and `strum` where they clarify known names.

## Tests and review gate

- Extend the existing table tests for unified mount, bind mount, namespace
  root, deleted membership, and ordering/fallback among matching mounts.
  Test candidate selection without requiring writable cgroups. Retain
  traversal and symlink protections in `handle.rs` and `io.rs`.
- Exercise the shared keyed-field path with CPU stat and PSI inputs covering
  reordered keys, unknown keys/kinds, absent optional groups, missing required
  values, malformed tokens, and unit overflow. Compare complete parsed
  structs. Do not create a new duplicate-key rule or test for one.
- Run `devenv test` (format, Clippy, tests, docs). Run Linux tests through the
  available Linux CI/container environment for the `cfg(target_os = "linux")`
  code. The VM suite is useful when available, but a QEMU boot failure before
  the test executable starts is an infrastructure failure, not a parser result.
- The reviewable diff should be limited to discovery, keyed parsing, their
  tests, and module wiring. Apart from the explicit `CgroupPathError` addition,
  do not change reader signatures or relocate public modules. Leave publishing
  configuration and new controller features out of this stage.

## Handoff to stage 2

Document any private helper names or error-mapping choices made here. Stage 2
will move the keyed parser and its public error types out of `cgroup/common`;
avoid creating public imports that make that move harder.
