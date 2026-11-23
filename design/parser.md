# Parser design research

## Status

The original parser-only research below evaluates whether Sakai's cgroup text
parsing is lightweight enough, whether parsing should be lazy, and whether a
general-purpose library would make the implementation clearer or more declarative.
The [read and write follow-up](#read-and-write-follow-up) evaluates a shared
declaration for the kernel text used by configuration writes.

The recommendation is:

1. Keep parsing eager and keep the small hand-written parser primitives.
2. Do not add `nom`, `serde`, `bytemuck`, `zerocopy`, or another parser
   dependency for the current formats.
3. Replace keyed-file tree maps with a fixed builder if profiling shows that
   parser cost matters.
4. Add a small cgroup-specific schema macro only after several controllers show
   stable, repeated boilerplate.
5. Reconsider `winnow` for genuinely nested formats if the hand-written parser
   becomes difficult to maintain.

This is consistent with the existing v0 decision not to use `serde` for cgroup
text. `serde` can remain an optional facility for already-parsed public types.

## Input characteristics

Cgroup v2 exposes small textual snapshot files rather than fixed-layout binary
records. The parser layer needs to support five shapes:

1. A single value, such as `memory.max`.
2. Positional whitespace-separated values, such as `cpu.max`.
3. Newline-separated values, such as `cgroup.procs`.
4. Unordered flat keyed values, such as `cpu.stat` and `memory.stat`.
5. Nested keyed values, such as PSI and later I/O files.

The kernel can add keys to keyed files. Consequently, those formats must be
matched by key, accept or retain unknown fields as appropriate, and must not
depend on line position. Parsing must also preserve enough source context to
produce useful errors and must perform checked unit conversion and range
validation.

These properties matter more than support for a complicated grammar. Most
individual tokens are ASCII identifiers, ASCII whitespace, decimal integers,
`max`, or `key=value` pairs.

## Current implementation

The positional parser uses `split_ascii_whitespace`, borrows tokens from its
input, parses integers with `FromStr`, performs checked unit conversion, and
verifies that no unexpected field remains. Successful parsing does not allocate
for tokenization or error context.

The current `cpu.stat` implementation makes one pass over the lines and stores
up to eight borrowed values in a `BTreeMap`. It then looks up required fields and
converts their values. This is straightforward and remains small in absolute
terms, but the tree is more general than the problem requires. Its nodes allocate
on the heap and its lookups are logarithmic even though the set of known keys is
fixed and tiny.

Errors own their raw input and invalid token so that an error can outlive the
source buffer. Those allocations occur only on failure.

## Performance observation

A release-mode scale check was run on 2026-09-19 on an Apple M1 Max with Rust
1.98.1. It repeatedly invoked the public `FromStr` implementations and used
`std::hint::black_box` around inputs and results.

| Operation | Approximate time |
| --- | ---: |
| Parse a two-field `cpu.max` fixture | 2.27 microseconds |
| Parse an eight-field `cpu.stat` fixture | 3.15 microseconds |
| Read a cached regular file and parse `cpu.stat` | 15.5 microseconds |

This was a local smoke benchmark rather than a statistically rigorous Criterion
suite. The regular-file result is not a substitute for measuring cgroupfs on
Linux. It establishes the order of magnitude and should not be treated as a
cross-platform performance guarantee.

At 1,000 eight-field parses per second, 3.15 microseconds per parse represents
about 3.15 milliseconds of CPU time per second, or roughly 0.3% of one core.
Typical snapshot reads should therefore treat parsing as negligible relative to
opening and reading the kernel interface. Workloads polling tens of thousands of
cgroups per second should benchmark the complete Linux read path before relying
on that assumption.

If complete-path performance becomes important, likely targets include:

- avoiding the `BTreeMap` for fixed known keys;
- reusing read buffers where the I/O API permits it;
- avoiding repeated open/read/close cycles where file descriptor semantics
  permit reuse;
- reducing conversion work only after measurement identifies it as material.

Changing the parser framework is unlikely to reduce kernel I/O cost.

## Laziness

There are three different ideas commonly called lazy parsing:

1. **Streaming input:** parse a partial buffer and request more bytes.
2. **Lazy iteration:** produce one parsed record at a time.
3. **Deferred conversion:** retain raw tokens and convert a field when an
   accessor first requests it.

Parser-combinator streaming support addresses the first case. Cgroup interface
files are already read completely into a `String`, are generally small, and are
consumed as snapshots, so streaming input offers little benefit.

Lazy iteration can be useful for unbounded lists, but it is not useful for a
two-field `cpu.max` or an eight-field `cpu.stat`. It may later be appropriate for
files such as `cgroup.procs`, depending on the public API.

Deferred conversion would require result values to retain the original source
through a lifetime or shared allocation. It would also move parse failures from
construction into accessors, complicate caching and thread-safety decisions, and
possibly repeat failed conversions. Most callers of the typed snapshot structs
need several fields, so little work would be avoided.

`nom`, `winnow`, and `pest` do not make the returned domain object lazy by merely
being adopted. Laziness would be a data model and public API decision independent
of the syntax parser.

## Library evaluation

### `nom`

`nom` is capable of parsing every required format and can borrow slices from the
input. Its combinators are useful when a grammar has meaningful nesting,
alternatives, and backtracking behavior.

For single values and positional whitespace-separated fields, its combinator
expressions would replace a few standard-library calls without removing any
domain validation. For unordered keyed files, the implementation would still
need a fold or dispatch step, duplicate handling, grouped-field validation,
unknown-key policy, typed unit conversion, and contextual error translation.

Adopting `nom` would therefore add an abstraction and dependency without making
the current domain structs lazy or substantially more declarative.

### `winnow`

[`winnow`](https://docs.rs/winnow/latest/winnow/) is the strongest general parser
candidate if the syntax layer becomes more complex. It provides composable text
parsers, contextual errors, conversion through `FromStr`, iterator-style repeated
parsing, and a `seq!` macro that can initialize structs from positional syntax.
It explicitly aims to support both declarative and imperative parsing styles.

It would express `cpu.max` and nested `key=value` records cleanly. Unordered flat
keyed records would still require dispatching or folding parsed pairs into a
builder. For the formats currently implemented, that is not clearer than
`lines`, `split_ascii_whitespace`, and `match`.

Reconsider `winnow` when implementing PSI, `io.stat`, mountinfo, or other nested
formats if their parsers accumulate enough manual cursor and error-management
code to obscure the grammar.

### `pest`

[`pest`](https://pest.rs/book/grammars/grammars.html) compiles a Parsing
Expression Grammar into Rust parser code. It provides the most visibly formal
syntax specification among the evaluated choices.

The result is a tree or iterator of grammar-rule pairs, not a completed Sakai
domain object. Application code must still walk those pairs, dispatch keys,
parse integers, construct `uom` values, validate grouped optional fields, and
translate errors. For these small kernel records, this introduces an extra
grammar and parse-tree layer. Its PEG matching is eager, so it does not provide
deferred domain-field conversion.

`pest` is better suited to a language or configuration format with a grammar
large enough to justify a separate grammar artifact.

### `serde`

`serde` is the only evaluated general library that could make named field mapping
meaningfully more declarative. A custom cgroup deserializer could present a flat
keyed file through [`MapAccess`](https://docs.rs/serde/latest/serde/de/trait.MapAccess.html),
allowing structs to derive `Deserialize`:

```rust,ignore
#[derive(Deserialize)]
struct RawCpuStat {
  usage_usec: Microseconds,
  user_usec: Microseconds,
  system_usec: Microseconds,
  #[serde(default)]
  nr_periods: Option<Count>,
  #[serde(default)]
  nr_throttled: Option<Count>,
}
```

Derive could provide field-name dispatch, required and optional field handling,
duplicate detection, and an unknown-field policy. Borrowed deserialization could
avoid copying successful input tokens.

However, Serde explicitly states that it is not itself a parsing library and
that a format implementation remains responsible for parsing its input. See
[Writing a data format](https://serde.rs/data-format.html) and
[Implementing a deserializer](https://serde.rs/impl-deserializer.html). Sakai
would still need to implement line and token parsing, numeric conversion, raw
source retention, unit-aware wrappers, grouped-field validation, and the
different positional, list, flat, and nested formats.

The custom deserializer is likely to exceed the hand-written code it replaces at
the current project size. It becomes more attractive if the project eventually
contains many similarly shaped keyed structs and field-mapping boilerplate is a
larger maintenance cost than the format adapter.

For v0, retain the existing design decision: use `serde` only as a possible
optional serialization layer for values that Sakai has already parsed.

### `bytemuck` and `zerocopy`

[`bytemuck`](https://docs.rs/bytemuck/latest/bytemuck/) casts between plain data
types and their byte representations. [`zerocopy`](https://docs.rs/zerocopy/latest/zerocopy/)
provides typed views over byte slices for compatible binary layouts.

Neither applies to cgroup text. The bytes in `54321` are five ASCII digit bytes,
not the native-memory representation of the integer 54,321. Keys and values are
variable length, lines can be reordered, and kernel versions can insert fields.
Every implementation must inspect delimiters and perform decimal conversion.
Overlaying a Rust struct on the source bytes would be semantically incorrect.

### Numeric and scanning libraries

Libraries such as `lexical-core`, `atoi`, `memchr`, or `bstr` can optimize narrow
parts of the implementation. They do not define the record schema or make field
mapping declarative. They should be considered only if profiling attributes a
material portion of runtime to decimal conversion or delimiter scanning.

### A cgroup-specific macro

A local schema macro could provide the most declarative interface because it can
encode the actual cgroup rules rather than a general syntax model. A possible
shape is:

```rust,ignore
cgroup_keyed_file! {
  CpuStat {
    required usage_usec: Time as Microseconds,
    required user_usec: Time as Microseconds,
    required system_usec: Time as Microseconds,

    group bandwidth {
      nr_periods: Count,
      nr_throttled: Count,
      throttled_usec: Time as Microseconds,
    }
  }
}
```

Such a macro could generate key dispatch, a fixed builder, missing and duplicate
checks, typed conversion, and consistent contextual errors. It could avoid the
generic `BTreeMap` while retaining key-order independence.

The cost is a repository-specific language, more difficult compiler diagnostics,
and macro implementation and testing. Do not introduce it based on one or two
types. First implement enough CPU, memory, PSI, and core files to identify which
patterns are actually stable.

## Recommended parser architecture

Keep a small two-layer design:

1. **Format scanners** recognize the shared physical shapes: single, positional,
   newline list, flat keyed, and nested keyed. They borrow slices and attach raw
   source context to syntax errors.
2. **Typed builders** dispatch known keys, enforce required and grouped fields,
   parse decimal values, perform checked unit conversion, and retain extras when
   the public type requires them.

For fixed keyed schemas, prefer a struct or fixed array of `Option<&str>` over a
tree map. Use a map only when the public result must preserve an open-ended set
of unknown keys.

Maintain these behavior requirements regardless of implementation technique:

- match keyed files by key rather than line number;
- tolerate kernel-added fields according to each file's policy;
- reject missing, duplicate, malformed, and partially present grouped fields;
- reject excess positional fields;
- perform checked numeric and unit conversion;
- preserve useful raw-line or raw-file context in errors;
- keep parsers platform-independent so their tests run on macOS and Linux.

## Reconsideration triggers

Revisit this decision when at least one of these conditions is observed:

- profiler data from a representative Linux workload shows parsing consuming a
  material share of CPU;
- nested parsers contain enough manual cursor logic that their grammar is no
  longer evident from the code;
- five or more keyed structs repeat essentially identical dispatch, missing,
  duplicate, and conversion code;
- a public API needs incremental iteration over a potentially large interface
  file;
- downstream users require `serde` support for already-parsed values.

Until then, the standard-library parser is smaller, easier to audit, and better
aligned with the textual kernel ABI than the evaluated alternatives.

## Read and write follow-up

Use one declaration for each text shape that can be shared in both directions.
The declaration owns field order or key names and associates each field with
its token codec. Generate the parser and encoder from that declaration.
Associate the kernel filename and operation types with a separate interface
descriptor. A writable interface can have a different command type from its
snapshot type.

For values representable in a shared shape, decoding the encoded value should
recover the same value. The encoder can choose a canonical spelling while the
decoder accepts extra ASCII whitespace and other compatible spellings. Errors
for unrepresentable values belong in a fallible encoder. For example, the
current `NonZeroTime` can represent a duration with a sub-microsecond remainder.
Truncating that remainder changes the value, and returning a formatting error
from `Display` makes `to_string()` panic. Only derive `Display` for wire types
whose representation is guaranteed by construction. [Rust `ToString`
documentation](https://doc.rust-lang.org/std/string/trait.ToString.html#panics)

The text round trip applies in memory. The kernel can reject settings based on
the cgroup's configuration, normalize values such as page-aligned memory
limits, or treat a write as an action. [Linux v2 interface
documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#interface-files)

### The shared shape varies by interface

| Interface | Snapshot | Command Sakai can support |
| --- | --- | --- |
| `memory.max` | One byte limit or `max` | Same scalar codec. |
| `cpu.max` | Quota and period in microseconds | Same positional codec for a full replacement. Optionally add a quota-only command using the same quota codec. |
| `cpu.stat` | Unordered keyed counters with optional groups | Read-only. Keep field-name decoding and group validation. |
| `io.max` | Unordered device rows with all four known limits per row | One device and a nonempty subset of fields. Share the key and unit declaration. Omission leaves the setting alone, while `max` clears a limit. |
| `cgroup.procs` | A list of process IDs | One process ID per write. The task ID codec can be shared. |
| `cpu.pressure`, `io.pressure`, `memory.pressure` | Nested keyed PSI statistics | A trigger command with a threshold and window. The open descriptor owns the trigger. |

[Linux v2 interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#interface-files)
and [Linux PSI documentation](https://docs.kernel.org/accounting/psi.html#monitoring-for-pressure-thresholds)
describe these read and write contracts.

A value can also be used by several files. For example, `CpuList` is the
snapshot type of both `cpuset.cpus` and `cpuset.cpus.effective`. Bind filenames
and access to interface marker types so the writable interface can use the
same value as the read-only one. Parameterized files such as
`hugetlb.<hugepagesize>.max` can resolve the filename from a typed selector.

### Option 1 — Generate both directions with a local schema macro

Declare field names and token codecs once. The following sketches use
`macro_rules!` at item scope, where the macro can generate the concrete impls.

```rust,ignore
pub struct CpuMax {
  quota: MaxOr<NonZeroTime>,
  period: NonZeroTime,
}

cgroup_value! {
  positional CpuMax {
    quota: Limit<NonZeroMicroseconds, MaxText>,
    period: NonZeroMicroseconds,
  }
}
```

The positional macro expands over the same field list to construct the struct
with `Parser::next_field` and encode its fields in that order. Start from the
existing encoding markers and `ParseCgroup<Unit>`. Pair a token's parse and
encode operations in the same codec, reusing the checked conversion and
sharing its scale and sentinel constants. Keep the existing public `FromStr`
error types and raw-input diagnostics. Have the formatter return a complete
validated command before it reaches I/O.

For nested keyed records, generate a field matcher and an emitter from one
list. Use separate enclosing rules for full snapshots and sparse patches.

```rust,ignore
cgroup_value! {
  keyed IoMaxFields {
    "rbps" => read_bandwidth: Limit<BytesPerSecondToken, MaxText>,
    "wbps" => write_bandwidth: Limit<BytesPerSecondToken, MaxText>,
    "riops" => read_iops: Limit<OperationsPerSecondToken, MaxText>,
    "wiops" => write_iops: Limit<OperationsPerSecondToken, MaxText>,
  }
}
```

The shared matcher can fill a builder with one slot per known field. The
snapshot adapter requires all four fields before constructing `IoDeviceMax`
and applies the existing unknown-key policy. The command adapter emits only
the fields present in a validated device patch. Preserve the current parser's
behavior during migration, including its duplicate policy. Today the
`BTreeMap` collection in `KeyedFields` retains the last value for a duplicate
key. Sakai can reject duplicates in newly introduced command input.

The smallest viable macro covers scalar and positional values. Add the keyed
arm when introducing the first per-device write. A procedural attribute
could read field names directly from the struct if repeating them starts to
cause friction. The declarative prototype shows that a proc-macro crate is
optional.

### Option 2 — Use `parse-display` for fixed textual records

```rust,ignore
#[derive(parse_display::FromStr, parse_display::Display)]
#[display("{quota} {period}")]
struct CpuMaxWire {
  #[display(with = Quota)]
  quota: MaxOr<WholeMicroseconds>,
  #[display(with = Microseconds)]
  period: WholeMicroseconds,
}
```

Both derives consume the same template. A shared field adapter can implement
`FromStrFormat` and `DisplayFormat`. With validated wire types, formatting is
infallible. The prototype on 0.11.0 accepted canonical `cpu.max` text but
rejected a tab separator, repeated spaces, or a terminal newline with the
literal-space template. Whitespace normalization restored those inputs. An
independent parse regex is also possible, at the cost of a second grammar.
The derive returns the crate's parse error. Preserving Sakai's existing
field-specific errors needs an adapter or a change to the public contract.
Unordered keyed fields and conditional updates still need cgroup-specific
logic. This is an option for a small family of fixed records if normalization
and the error contract are acceptable. [`parse-display` derive
documentation](https://docs.rs/parse-display/latest/parse_display/)

### Option 3 — Derive both directions with Serde on wire records

```rust,ignore
#[derive(serde::Serialize, serde::Deserialize)]
struct IoMaxPatchWire {
  #[serde(rename = "rbps", default, skip_serializing_if = "Option::is_none")]
  read_bandwidth: Option<BandwidthLimitWire>,
  #[serde(rename = "wiops", default, skip_serializing_if = "Option::is_none")]
  write_iops: Option<IopsLimitWire>,
  // The remaining limits follow the same pattern.
}
```

Derive shares key names and implements field dispatch. A cgroup
deserializer would feed keyed input through `MapAccess` and positional input
through `SeqAccess`. A cgroup serializer would control whitespace and
punctuation and emit the text of one command. The token wrappers or field
adapters still need checked units and sentinel semantics. Wire records let
Sakai keep these choices independent of any Serde representation exposed
to users for JSON or other formats.

A two-field keyed prototype used `MapDeserializer` over borrowed tokens
and a narrow serializer for `key=value` output. The derives handled
reordered keys, ignored an extra key, rejected a duplicate known key, and
omitted an absent field. The prototype needed token conversion and a format
adapter for both directions even with only two fields. Serde also permits
direction-specific names and hooks, so restricting the schema to shared
declarations is a project convention. Revisit this option if enough keyed
layouts can share the same format adapter. [Serde data model](https://serde.rs/data-model.html)
and [field attributes](https://serde.rs/field-attrs.html)

The ecosystem also has composable parser/printer pairs. For example,
`id_effect_parse` 0.4.0 exposes a `codec::pair` function and a `Codec::map` with two
conversion functions. Its primitive codec accepts independent parse and
print functions, its printer is infallible, and its CC-BY-SA-4.0 license is
outside the workspace allowlist. `binrw` and `deku` are examples of one
declaration generating both directions for binary layouts. `nom`, `winnow`,
and `pest` can still help with scanning but leave the printer to Sakai.
[`id_effect_parse` codec source](https://docs.rs/crate/id_effect_parse/0.4.0/source/src/codec.rs),
[`binrw`](https://docs.rs/binrw/latest/binrw/), and
[`deku`](https://docs.rs/deku/latest/deku/)

### Typed file access and Rust trait generation

```rust,ignore
trait CgroupFile {
  const FILE_NAME: &'static str;
  type Access: mode::Sealed;
}

struct CpuMaxFile;
impl CgroupFile for CpuMaxFile {
  const FILE_NAME: &'static str = CpuMax::FILE_NAME;
  type Access = mode::ReadWrite<CpuMax, CpuMax>;
}

struct IoMaxFile;
impl CgroupFile for IoMaxFile {
  const FILE_NAME: &'static str = IoMax::FILE_NAME;
  type Access = mode::ReadWrite<IoMax, IoMaxPatch>;
}
```

Use sealed `ReadOnly<S>`, `WriteOnly<C>`, and `ReadWrite<S, C>` markers.
Read helpers require a `Readable` bound with an associated snapshot type.
Write helpers require a `Writable` bound with an associated command type.
A mode enum constant can describe access for introspection, but the marker
bounds enforce it at compile time. File availability and permissions are
still checked by the kernel. Keep write I/O behind the controller API and
use the existing directory-relative path handling. Send each fully encoded
command in a single write and treat a short write as an error. PSI triggers
and other descriptor-scoped commands can use dedicated methods.

Rust's orphan rules reject blanket implementations such as
`impl<T: CgroupValue> FromStr for T` and the equivalent `Display` impl.
An item macro or derive can generate those impls per concrete type. A
local wrapper such as `Wire<T>` can also carry blanket standard-trait
impls where the value and error contracts fit. A `shape(self)` method
requires a value before decoding can begin. A type-level declaration can
construct that value from input and later project its fields by reference.
The macro can do both from field names. [Rust trait coherence
rules](https://doc.rust-lang.org/reference/items/implementations.html#trait-implementation-coherence)

### Experiments and implementation order

Temporary experiments on Rust 1.100.0 (macOS) passed six tests across the
three options. The local macro covered whitespace and canonical output,
`max` and numeric quota, integer overflow, missing and excess fields,
unrepresentable nanoseconds, and unordered keyed fields and sparse updates.
A cgroup v1 quota prototype reused the same microseconds codec with a
`-1` sentinel. The kernel exposes the v1 period through a separate file,
so the v2 `cpu.max` record itself stays version-specific. Compile-fail
probes confirmed the orphan-rule errors and rejection of a write to a
read-only descriptor. These were text and type-checking experiments,
without live kernel writes. [Linux v1 CPU bandwidth
documentation](https://docs.kernel.org/scheduler/sched-bwc.html#management)

Start with option 1 for a full `cpu.max` replacement and a scalar such as
`memory.max`. Reuse fixtures to check the existing parser behavior.
Test the shared codec's canonical bytes against kernel examples as well
as its value round trip. A round-trip test alone can pass if both generated
implementations agree on the same wrong field order, unit, or key spelling.
Use Linux integration tests to check accepted writes and readback in a
delegated cgroup. Add `io.max` when the write API needs sparse patches,
including tests for omitted versus explicitly cleared fields and for
group or device validation. Keep complex read-only parsers on their existing
scanners until they have a concrete reason to use a shared declaration.
