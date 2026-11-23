---
status: accepted
date: 2026-11-24
---

# Declare shared cgroup text codecs

## Context and Problem Statement

Sakai parses cgroup snapshots into typed values. Configuration writes also need
to encode values in the kernel's text format. Independent implementations can
disagree about field order, keys, units, and sentinel tokens. Some interfaces
share a text shape for reads and writes, while others use different shapes
for snapshots and commands. [Linux cgroup v2 interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#interface-files)

## Decision Drivers

- Declare each shared field's position or key and encoding once.
- Keep declarations short and preserve existing typed values, accepted input,
  and field-specific parse errors.
- Preserve units exactly and allow the same token codecs to be used across
  interfaces and cgroup versions.
- Express command-specific semantics without expanding the text grammar.

## Considered Options

- A local declarative macro with reusable token codecs.
- Shared format templates from `parse-display`.
- Serde derives over an internal wire model and custom text adapters.
- Hand-written `FromStr` and encoder implementations per interface.

## Decision Outcome

Use a crate-local `cgroup_value!` macro where a declaration drives both
struct construction and field projection. Positional records, keyed fields,
and homogeneous maps declare their fields and token encodings once. Scalars
reuse the encoding marker already selected by `Scalar`. Formats with
interface-specific validation compose those codecs in ordinary Rust.

Reuse the existing parsers and emit canonical text without a trailing newline.
Encoding preserves the typed snapshot, including optional fields, and reports
values that cannot be represented exactly. For example, a duration with a
sub-microsecond remainder cannot be encoded in whole microseconds. An
`io.stat` row containing only unknown counters has no retained fields to
encode. Text encoding is independent of file access and kernel validation.

### Consequences

- Setters can use the encoder without defining the wire format again.
- Sakai owns a small macro and its diagnostics. Add another grammar arm
  when a concrete interface needs it.
- When a snapshot and command differ, share the applicable token or field
  declaration and express the enclosing rules in Rust. For example, an
  `io.max` snapshot lists devices and all four limits per row. An update
  applies to one device and can omit limits that should retain their current
  settings. The v1 CPU quota can reuse microseconds with a distinct
  unlimited sentinel. [Linux v2 I/O interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files) · [Linux v1 CPU bandwidth control](https://docs.kernel.org/scheduler/sched-bwc.html#management)

### Confirmation

Check exact canonical strings alongside value round trips. An independently
specified string catches cases where both generated directions agree on the
same wrong field order or unit. Keep the existing parse fixtures and error
checks to verify compatibility. Test file writes separately against the kernel
when they are implemented.

## Pros and Cons of the Options

### A local declarative macro with reusable token codecs

- Shares field order and unit selection while reusing the parser and its
  errors. The positional prototype handled whitespace, checked conversion,
  and both finite and unlimited quotas.
- Introduces a local abstraction that needs maintenance and can make
  compiler errors harder to interpret.

### Shared format templates from `parse-display`

- A compact template drives parsing and formatting for fixed records.
- The prototype required whitespace normalization for existing `cpu.max`
  inputs. Field-specific diagnostics and unordered or sparse records need
  additional adapters and conventions. [`parse-display` documentation](https://docs.rs/parse-display/0.11.0/parse_display/)

### Serde derives over an internal wire model and custom text adapters

- The derives share field names and mapping. The keyed prototype accepted
  reordered keys and omitted absent fields on encoding.
- Sakai still owns both text adapters, including delimiters and token
  conversions. Shared Serde attributes alone leave room for those adapters
  to drift. [Serde data model](https://serde.rs/data-model.html)

### Hand-written implementations per interface

- Small, direct Rust implementations work well for one-off formats.
- Each shared field order, key, and encoding must be kept in sync across
  the two implementations.
