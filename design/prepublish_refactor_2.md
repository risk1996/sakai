# Prepublish refactor 2: shared modules and error provenance

## Goal and place in the sequence

Start after [`prepublish_refactor_1.md`](prepublish_refactor_1.md) is complete
and reviewed. Move cross-controller concepts out of `cgroup/common`, simplify
owned parse errors, and retain their source at the Linux read boundary. This
stage intentionally changes some public type paths and error shapes, but does
not yet replace `ReadCore` or `ReadCpu`; that is stage 3.

The current library exposes OS-independent `FromStr` parsers and Linux-only
`Cgroup` I/O. Follow `AGENTS.md`, `CODE_STYLE.md`, and `RTK.md`; preserve user
changes. The user has explicitly approved this prepublish API restructuring.

## Target ownership and public paths

Use this module ownership (subfiles are optional when a small module suffices):

```text
sakai-core/src/
  lib.rs                # short public re-exports and crate documentation
  unit.rs               # BaseUnits, Time, NonZeroTime, CountKind, Count,
                        # EventRate, Ratio
  limit.rs              # MaxOr<T>, the cgroup `max` sentinel
  pressure.rs           # Pressure and PressureLine, shared PSI value types
  error.rs              # Error, ParseError, ParseValueError, FieldKind
  parse/                # private positional and keyed parsing machinery
  cgroup/v2/            # core/cpu values, Linux discovery, handle, I/O
```

Quantities belong in `unit`, but `MaxOr<T>` is a limit representation, not a
unit, and `Pressure` is a snapshot, not a unit. `Weight` and `Nice` stay under
`v2::cpu` because their valid ranges and meaning are CPU-specific. Preserve
the root aliases for `Time`, `Count`, `EventRate`, `Ratio`, `NonZeroTime`,
`MaxOr`, `Pressure`, `PressureLine`, `Error`, `Cgroup`, and `v2` as appropriate
for the platform. `FromStr` on CPU and PSI values must remain usable on macOS.
Remove the `v2::cpu::Pressure` re-export when migrating in-repository uses;
the root `Pressure` path is the shared public entry point.

Currently these live under [`cgroup/common`](../sakai-core/src/cgroup/common):
`unit/uom.rs`, `unit/max_or.rs`, `pressure.rs`, `parser.rs`, and `error.rs`.
[`lib.rs`](../sakai-core/src/lib.rs) re-exports them through that nesting.
After moving imports and tests, remove the obsolete `common` module rather
than maintaining two canonical locations. Make parser implementation types
crate-private; publicly expose only types required to name a public parse
result, especially `ParseError`, `ParseValueError`, and `FieldKind`.

## Error design

- [`ParseError<'a, E>`](../sakai-core/src/cgroup/common/error.rs) currently
  uses `Cow<'a, str>`, while every parser returns `ParseError<'static, E>` and
  its constructors allocate owned strings. Replace it with `ParseError<E>`
  containing owned raw input and offending values. Keep distinct missing,
  excess, and invalid cases, the field name, and the underlying typed source.
  `Box<str>` or `String` is acceptable; prefer one clear owned representation.
- [`Error::Parse`](../sakai-core/src/cgroup/common/error.rs) currently stores
  only a static file name and a rendered `detail: String`, losing the parser
  error as a source. Change it to carry the full interface path and a boxed
  error source (`Send + Sync + 'static`) with `#[source]`. Update
  `Cgroup::parse` and the special `cgroup.type` reader to attach the original
  error. Retain `FileMissing { path }` and all other I/O classifications.
- Keep human-readable diagnostics with the raw kernel content and field
  context. Preserve `std::error::Error::source()` chains. Verify actual
  `FromStr::Err` bounds for all public parser types before applying a blanket
  `Cgroup::parse` bound; adapt the one-off `cgroup.type` path explicitly if
  its `strum` error needs a wrapper.

## Migration and tests

1. Move the shared value types and update their imports/re-exports, rustdoc
   links, doctests, README examples, and integration tests. Keep the existing
   `uom` storage choices, checked microsecond-to-nanosecond conversion,
   `nutype` validation, and `MaxOr::Max` semantics. Do not redefine these
   values as primitive integers/floats.
2. Move the positional parser and stage-1 keyed helper into `parse`; move
   their public error types into `error`. Update every `FromStr::Err`, parser
   marker import, and test. Keep successful parse values unchanged.
3. Change the Linux read error boundary and test that a parse failure reports
   the interface path and exposes its underlying parse error as a source.
   Test `FileMissing` separately so missing old-kernel interfaces do not
   become parse failures. Retain exact parser diagnostics where callers rely
   on them; update assertions only for intentional error-shape changes.
4. Run `devenv test` and Linux tests. Review public rustdoc on both platforms:
   aliases and error types must be reachable without importing a private
   module. No publishing configuration, controller-view API, or new metrics
   are in scope for this stage.

## Handoff to stage 3

Stage 3 may assume `crate::{error, limit, pressure, unit}` are the canonical
shared paths and that `Cgroup::parse` preserves a typed source. It should
only need to alter how controller readers are reached and what the four
single-value CPU methods return.
