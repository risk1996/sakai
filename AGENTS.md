## Coding style

Working with Rust, see @CODE_STYLE.md

## Decision tracking

When asked to create ADR, follow https://adr.github.io/madr/.

## Tool invocation

- Project tools: `devenv shell -- rtk <command>`.
- Checks: `devenv test` or `devenv tasks run check:<fmt|clippy|test|doc>`.
- Other commands, including `sed`: run `rtk <command>` directly, following @RTK.md. Wrap executables, not shell built-ins/operators. Do not use `devenv shell` for these commands; it can require Nix daemon access even when the command does not.
- If sandboxing blocks the Nix daemon, rerun `devenv` with elevated access.

## Code review

- After every commit, run `devenv shell -- rtk coderabbit review --agent --committed` and inspect the findings before handing off the change. `--committed` limits the review to committed changes.
- Commit any fixes and review again after that commit. If review cannot complete (for example, authentication or network access is unavailable), report the failure and do not claim the review passed.
- If authentication is needed, use `devenv shell -- rtk coderabbit auth login --agent`.
