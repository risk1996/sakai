# Rust

- Ground the design based on the domain:
  - in this case, the cgroup v2 kernel interface and its semantics.
- Follow the existing code for reference and keeping the code style consistent.
- Place all `use` statements at the top of their module, including test modules; never put them inside functions.
- Prioritize function purity and immutability, avoid side effects, use declarative and functional paradigm instead of imperative, avoid `let mut` whenever possible.
- Leave formatting to the project formatter; verify with `devenv tasks run check:fmt`.
- Prioritize correctness and leveraging the awesome Rust type system:
  - use enum instead of string matching if the list of possible values is known and finite,
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

## Note

If you find contradictions, these are the priority (lower ones should follow the above):

- The latest instructions given
- This code style guide
- The existing code in the repository
