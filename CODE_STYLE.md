# Rust

- Ground the design based on the domain:
  - in this case, the cgroup v2 kernel interface and its semantics.
- Follow the existing code for reference and keeping the code style consistent.
- Prioritize function purity and immutability, avoid side effects, use declarative and functional paradigm instead of imperative, avoid `let mut` whenever possible.
- Leave formatting to `devenv task check:fmt`.
- Prioritize correctness and leveraging the awesome Rust type system:
  - use enum instead of string matching if the list of possible values is known and finite,
  - use `uom` whenever there is a dimension of the return type (e.g. time, bytes, count, etc.), say no to primitive obsession,
  - use `nutype` for creating a newtype whenever you need validation.
- Be concise, avoid repetition, automate as much as possible:
  - use `strum` for common operations with enums.
- Avoid free-floating functions, `impl` on related struct or enum instead.
- Use `match` instead of `if` whenever possible.
- Tests:
  - avoid creating multiple tests for similar outcomes, use table test instead,
  - use `indoc!` for multi-line strings, move to fixtures and `include_str!` if it surpasses 20 lines,
  - construct the expected struct and assert equality with the parsed struct, not asserting field-by-field.

# Shell Scripts

- The ideal number of shell script lines is zero. Write Rust code instead of Bash scripts whenever possible.
- Bash is OK for a one-off job, but should never considered to be something committed to the repository.

# Commit

Follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

## Note

If you find contradictions, these are the priority (lower ones should follow the above):

- The latest instructions given
- This code style guide
- The existing code in the repository
