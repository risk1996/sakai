## Coding style

Follow @CODE_STYLE.md for coding and documentation conventions, including
public Rust API docs and Python docstring tests.

## Decision tracking

When asked to create ADR, follow https://adr.github.io/madr/.

## Tool invocation

- Project tools: `devenv shell -- rtk <command>`.
- Checks: `devenv test` or `devenv tasks run check:<fmt|clippy|test|doc>`.
- Other commands, including `sed`: run `rtk <command>` directly, following @RTK.md. Wrap executables, not shell built-ins/operators. Do not use `devenv shell` for these commands; it can require Nix daemon access even when the command does not.
- If sandboxing blocks the Nix daemon, rerun `devenv` with elevated access.

## Codex Code review

- For local work before a pull request is open, run
  `devenv shell -P review -- rtk codex review --commit HEAD` after each
  commit. Inspect the findings before handing off the change. Commit any
  fixes and review again. If the review cannot complete, report the failure.
- If local authentication is needed, use
  `devenv shell -P review -- rtk codex login`.
- In CI and once a pull request is open, rely on the automatic Codex Code
  review configured outside this repository.
