## Coding style

Working with Rust, see @CODE_STYLE.md

## Tool invocation

- Project tools: `devenv shell -- rtk <command>`.
- Checks: `devenv test` or `devenv tasks run check:<fmt|clippy|test|doc>`.
- Other commands: follow @RTK.md. Wrap executables, not shell built-ins/operators.
- If sandboxing blocks the Nix daemon, rerun `devenv` with elevated access.
