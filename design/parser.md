# Parser design research

## Status

This note evaluates whether Sakai's cgroup text parsing is lightweight enough,
whether parsing should be lazy, and whether a general-purpose library would make
the implementation clearer or more declarative.

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
