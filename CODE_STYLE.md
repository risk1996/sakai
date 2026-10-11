# Rust

- Ground the design based on the domain:
  - in this case, the cgroup v2 kernel interface and its semantics.
- Follow the existing code for reference and keeping the code style consistent.
- Place all `use` statements at the top of their module, including test modules; never put them inside functions.
- Prioritize function purity and immutability, avoid side effects, use declarative and functional paradigm instead of imperative, avoid `let mut` whenever possible.
- Use `&self` for read-only instance methods, including getters, predicates,
  and derived-metric convenience functions, even when the receiver is `Copy`.
  Borrowing expresses inspection and avoids whole-receiver copies without
  relying on inlining or release optimizations; those copies can remain in
  debug builds and calls across crate boundaries. The convention also remains
  suitable as snapshots grow or gain non-`Copy` fields. Return small `Copy`
  quantities by value. Use `&mut self` when mutating the receiver and `self`
  when deliberately consuming or transferring ownership, such as `into_*`
  conversions.
- Leave formatting to the project formatter; verify with `devenv tasks run check:fmt`.
- Prioritize correctness and leveraging the awesome Rust type system:
  - use enum instead of string matching if the list of possible values is known and finite,
  - mark public enums `#[non_exhaustive]` when kernel fields, controllers, or
    error categories can grow. Keep genuinely closed alternatives exhaustive.
    Use exhaustive matches inside the defining crate so additions require
    updating internal handling; downstream matches need a fallback.
  - use `uom` whenever there is a dimension of the return type (e.g. time, bytes, count, etc.), say no to primitive obsession,
  - use `nutype` for creating a newtype whenever you need validation.
- Be concise, avoid repetition, automate as much as possible:
  - use `strum` for common operations with enums.
  - Put stable domain identifiers on their owning struct or enum as associated constants.
    Cgroup interface filenames are public `FILE_NAME` constants; shared result types
    use descriptive names such as `Pressure::CPU_FILE_NAME`.
  - Name meaningful numeric values and repeated operational strings, such as unit
    conversion factors, timeouts, and cache paths. Prefer typed quantities and
    the narrowest useful visibility; public constants must benefit callers.
  - Keep self-explanatory literals inline: parser field names, enum serialization
    names, validation ranges, zero/one, and test inputs and expected values.
    Tests may retain literal expectations to independently verify the contract.
  - Extract shared meaning, not merely equal values. Avoid a global constants
    collection, new abstractions, or generators for incidental repetition.
- Avoid free-floating functions, `impl` on related struct or enum instead.
- Use `match` instead of `if` whenever possible.
- Tests:
  - avoid creating multiple tests for similar outcomes, use table test instead,
  - use `indoc!` for multi-line strings, move to fixtures and `include_str!` if it surpasses 20 lines,
  - construct the expected struct and assert equality with the parsed struct, not asserting field-by-field.

# Documentation

- Document every public Rust function, struct, public struct field, enum, and
  enum variant. Keep public modules, traits, type aliases, and constants
  documented too, so the exported API has no missing rustdoc.
- Use plain language to explain behavior, units, valid ranges, and whether
  values describe this cgroup or its descendants. Explain snapshot volatility
  and that getters inspect stored snapshots while interface reads perform fresh I/O.
  Document errors for fallible readers, including missing interfaces, unsupported
  operations, invalid contents, and other operating-system failures. Distinguish
  missing optional counters from measured zero values.
- Link directly to the relevant authoritative Linux kernel documentation or
  Linux man page wherever one exists. For interfaces, fields, or behavior added
  after the crate's documented Linux compatibility baseline, include a
  "Since Linux x.y" note backed by a link to versioned upstream documentation
  or source. Verify introduction versions against release-tagged sources;
  account for kernel configuration, controller availability, root exemptions,
  and distribution backports when describing availability.
- Keep native Python docstrings in the Rust bindings (doc comments and exception
  declarations) and expose them through PyO3. In Python documentation tests, compare exactly one complete
  representative docstring: the top-level `Cgroup` class. For the module and
  all other exported classes, methods, and properties, check that documentation
  exists without matching its wording. Require each class's own nonempty
  `__doc__`; inherited `object` or `Exception` documentation must not satisfy
  that check. Rust documentation edits should not require updating Python text
  expectations beyond the single `Cgroup` example.
- Verify changed Rust docs with the project formatting and doctest checks,
  and build rustdoc with `-D missing_docs -D rustdoc::broken_intra_doc_links`.
  Include a Linux build when checking Linux-only APIs. Verify Python docstring
  changes against the installed package outside the checkout, retaining coverage
  of the exported API and its methods and properties.

# Shell Scripts

- The ideal number of shell script lines is zero. Write Rust code instead of Bash scripts whenever possible.
- Bash is OK for a one-off job, but should never considered to be something committed to the repository.

# Versions and Tooling

- Keep one authoritative source for each version or fixture list; derive consumers
  through existing configuration, build arguments, or Rust tooling.
- Container Rust versions follow the resolved toolchain in `devenv.nix`; vmtest
  follows its `devenv.yaml` input through the generated `devenv.lock`.
- Share release versions, editions, and repeated dependencies through Cargo's
  workspace tables. Python release metadata derives its version from Cargo.
- Development tool versions and minimum supported runtimes are separate decisions.
  Keep compatibility requirements explicit where packaging and ABI tools require them.
- Prefer version tags and compatible dependency ranges over manually maintained
  image digests or download checksums. Keep generated lockfiles and download
  concurrency/atomicity protections.

# Commit

Follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).
For implementation commits, the scope should be comma-separated programming languages,
e.g. `feat(rust,python): implement new feature`.

## Note

If you find contradictions, these are the priority (lower ones should follow the above):

- The latest instructions given
- This code style guide
- The existing code in the repository
