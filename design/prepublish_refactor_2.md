# Prepublish refactor 2: shared modules and error provenance

Completed stage 2. Shared quantities, limits, pressure snapshots, and errors
now live in `unit`, `limit`, `pressure`, and `error` at the crate root. The
positional and keyed parser helpers live in private `parse`; `cgroup/common`
and the `v2::cpu::Pressure` re-export were removed. Root aliases remain, and
CPU and PSI `FromStr` parsers work on macOS.

`ParseError<E>` now owns raw input and invalid values. Linux `Error::Parse`
keeps the interface path and the original error as a source, including for
`cgroup.type`; missing interfaces remain `FileMissing`. Parser diagnostics and
successful values are unchanged.

Verification passed: `devenv test` on macOS, 35 Linux library tests, Linux
doctests, and four live tests in a Linux 5.15 VM. Public rustdoc was generated
on both platforms.

Stage 3 can use `crate::{error, limit, pressure, unit}` as the canonical shared
paths and rely on `Cgroup::parse` preserving its source. `ReadCore` and
`ReadCpu` remain unchanged for that stage.
