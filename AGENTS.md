## Coding style

Working with Rust, see @CODE_STYLE.md

## Decision tracking

When asked to create ADR, follow https://adr.github.io/madr/.

## Change workflow

- Work on a branch specific to the change, not `main`. A Git worktree may be used.
- Commit changes with a Conventional Commit message.
- Do not push to remote. Pushing requires the user's YubiKey, which agents cannot access.
- When asked to create a PR, use @.github/pull_request_template.md as the template for the PR description, including the Assistance fields.

## Tool invocation

- Project tools: `devenv shell -- rtk <command>`.
- Checks: `devenv test` or `devenv tasks run check:<fmt|clippy|test|doc>`.
- Other commands, including `sed`: run `rtk <command>` directly, following @RTK.md. Wrap executables, not shell built-ins/operators. Do not use `devenv shell` for these commands; it can require Nix daemon access even when the command does not.
- If sandboxing blocks the Nix daemon, rerun `devenv` with elevated access.
