# Sakai releases

The shared version lives in `[workspace.package]` in the root `Cargo.toml`.
Rust and Python inherit it; Maturin derives Python metadata from Cargo.
Release preparation and registry uploads are separate responsibilities. Only
crates.io publishing is implemented here; no PyPI upload is configured yet.

Research checked on 2026-10-11. The key references are
[crates.io trusted publishing](https://blog.rust-lang.org/2025/07/11/crates-io-development-update-2025-07/),
[Cargo publishing](https://doc.rust-lang.org/cargo/commands/cargo-publish.html),
[GitHub workflow security](https://docs.github.com/en/actions/reference/security/secure-use),
[release-plz configuration](https://release-plz.dev/docs/config), and
[SecretSpec's environment provider](https://secretspec.dev/providers/env/).

## Responsibilities and credentials

`release-plz.toml` prepares a reviewable PR containing versions, the lockfile,
and the Rust/Python changelog. It follows `v<version>` Git tags rather than a
particular registry. The preparation workflow invokes **only** `release-pr`:
it never uploads packages, merges its PR, or creates a tag. There is no
preparation before the first stable tag, preserving the reviewed `0.1.0`
bootstrap version. Conventional Commits produce draft changelog entries;
reviewers must edit them into accurate user-facing notes and check Python
behavior as well as Rust compatibility. While below 1.0, compatible additions
and fixes can increment the patch version; breaking changes increment the
minor version. Automated compatibility checks do not replace review.

The release workflow runs the existing Linux CI, including MSRV, Python
packaging, security, Kubernetes, and Linux 5.15/6.18 smoke tests. It requires a
stable tag matching the workspace version, whose commit is on `origin/main`.
Preparation dry-runs Cargo publishing and preserves the archive, SHA-256,
source commit, and exact versioned changelog section as an Actions artifact.

The `publish-crates` job depends on those checks and uses the protected
`crates-io` environment. It repackages and compares the candidate before
authentication. Only this job receives `id-token: write`. The official
`rust-lang/crates-io-auth-action` exchanges OIDC identity for a temporary
crates.io token and revokes it in its post step. This is a registry API token,
not a GitHub PAT. Neither a long-lived registry token nor a secret-store
credential belongs in GitHub environment secrets for this flow.

SecretSpec is installed explicitly in devenv. The default profile makes the
token optional; shell entry, builds, and tests do not require one. Only the
upload command uses `secretspec run --provider env --profile publish-crates`.
The token is supplied to that step through the auth action's masked output,
not persisted with `cargo login` or exported through `GITHUB_ENV`. SecretSpec
does not provide OIDC authentication itself: it validates and supplies the
already-issued token. Secrets are not resolved during Nix evaluation.

The separate `announce` job has `contents: write`, but no OIDC permission.
It creates a GitHub release from the reviewed changelog only after the upload
is verified. The checked archive remains in Actions artifacts for 30 days;
the public distribution is crates.io. New registry jobs can depend on the
same checks/preparation, use their own environment and authentication, and
run independently. Add each to `announce.needs` so announcements wait for
every requested destination. Uploads across registries cannot be atomic:
retry failed destinations rather than trying to roll back immutable versions.

## One-time manual setup and first publication

Do not perform a production upload until these repository changes have been
reviewed, merged, and the merged commit's CI has passed. A published version
cannot be overwritten. Repeat candidate preparation after any source,
metadata, lockfile, or commit change, including a squash merge.

1. Protect the `crates-io` environment with required reviewer `risk1996`,
   administrator bypass disabled, and only the **tag** pattern `v*` allowed.
   A solo maintainer may leave self-review prevention disabled. Protect `v*`
   with the active release-tag ruleset: restrict creation, updates, deletions,
   and force pushes. Keep a narrow administrator bypass for manual tag
   creation; administrators using that bypass must never move release tags.
   The environment and tag ruleset are separate controls. No environment
   secrets are needed.
2. In repository Settings → Actions → General → Workflow permissions,
   allow GitHub Actions to create pull requests. Leave the default token
   permission read-only; the release-preparation job requests its write
   permissions explicitly. Organization policy may also need to allow this.
3. Log in to crates.io, verify the account's email, and confirm the intended
   name is still available. Create a short-lived API token scoped to `sakai`
   and `publish-new` only. Do not copy it into chat or GitHub secrets. Store it
   in Cargo's native macOS Keychain provider, not Cargo's plaintext token file:

   ```sh
   devenv shell -- rtk cargo login --registry crates-io --config 'registry.global-credential-providers=["cargo:macos-keychain"]'
   ```

   `cargo login` stores a credential; it does not publish anything. This
   native Cargo credential is intentionally separate from SecretSpec's
   generic keyring provider. No command in repository preparation reads it.
4. From a clean checkout of the final merged release commit, run the existing
   checks and prepare the candidate without a tag or publishing credentials:

   ```sh
   devenv test
   devenv tasks run check:security
   devenv shell -- rtk cargo package --package sakai --list --locked
   devenv shell -- rtk cargo xtask release prepare
   ```

   Inspect `dist/release/release.json`, `release-notes.md`, and the archive.
   Complete the final-commit gates in
   [the prepublication checklist](cratesio-prepublish-checklist.md), including
   extracted-package tests/docs. The preparation command uploads nothing.
5. **You**, not the agent, perform the first upload from that same clean commit:

   ```sh
   devenv shell -- rtk cargo publish --package sakai --locked --config 'registry.global-credential-providers=["cargo:macos-keychain"]'
   devenv shell -- rtk cargo xtask release verify-registry
   ```

   Do not pass `--allow-dirty`, `--no-verify`, or metadata-warning bypasses.
   Confirm the crate's owner, version, README, license, and docs.rs build.
   Keep the candidate report so the tag can identify the exact uploaded commit.
6. After that first upload, open the crate's settings on crates.io and add a
   GitHub Actions Trusted Publisher with these exact values:

   | Field | Value |
   | --- | --- |
   | Repository owner | `risk1996` |
   | Repository | `sakai` |
   | Workflow filename | `release.yml` |
   | GitHub environment | `crates-io` |

   The workflow filename is not the display name `Release`. An existing crate
   is required before its trusted publisher can be configured.
7. Revoke the bootstrap token on crates.io, then remove its local Keychain
   entry with the same provider configuration:

   ```sh
   devenv shell -- rtk cargo logout --registry crates-io --config 'registry.global-credential-providers=["cargo:macos-keychain"]'
   ```

   Logout alone does **not** revoke the server-side token. After a real OIDC
   upload succeeds, consider the crate's
   [trusted-publishing-only setting](https://blog.rust-lang.org/2026/01/21/crates-io-development-update/)
   to disable traditional API-token publishing. This would also require a
   different authentication design for future local Mac publishing.
8. Create the matching signed tag on the **exact commit already published**,
   and push it yourself using your YubiKey. For the initial version:

   ```sh
   rtk git tag -s v0.1.0 -m 'Release v0.1.0'
   rtk git push origin refs/tags/v0.1.0
   ```

   Wait for release CI, inspect the candidate artifact, and approve the
   `crates-io` deployment. The workflow accepts an already-published version
   only when its registry checksum matches exactly and it is not yanked.
   It skips authentication/upload for that bootstrap version and creates
   the GitHub release notes. A later new version is needed to exercise the
   actual OIDC upload end to end. If the Mac and Linux candidates differ,
   stop and investigate; do not disable the checksum check or repoint the tag.

## Subsequent releases and rehearsals

Merge ordinary changes with meaningful Conventional Commit messages.
Release preparation runs on `main` and opens or updates a release PR after
the first stable tag exists. It uses the job's `GITHUB_TOKEN`, not a personal
PAT. Follow GitHub's
[current token-trigger behavior](https://docs.github.com/en/actions/concepts/security/github_token):
bot-created PR checks may require approval. Approve/run the required CI before
merging; never merge a release PR merely because checks are missing.

Review the shared version, compatibility, and changelog; merge only after CI
passes. Create and push the corresponding tag yourself on the merged commit.
Tag pushes publish automatically after checks and environment approval.
Tags created by `GITHUB_TOKEN` normally do not trigger another workflow, which
is another reason the preparation job does not create them.

For a credential-free rehearsal, dispatch **Release** on an existing stable
version tag with `publish: false` (the default). It runs release checks and
preserves the candidate but never enters a publishing environment, requests
OIDC, uploads, or creates a GitHub release. Dispatching on a branch fails
candidate validation; there is no arbitrary checkout-ref override. Merely
pushing a new tag is **not** a rehearsal: it starts the publishing path.

If an upload succeeded but a later step failed, rerun the failed jobs for the
same tag. Artifact IDs preserve the original candidate across attempts.
Only an identical unyanked archive can be skipped. Post-upload verification
retries missing versions and transient network, timeout, rate-limit, and server
errors within a fixed attempt budget. Authorization errors, malformed metadata,
checksum conflicts, and yanked versions fail immediately; exhausted retries
retain useful diagnostics rather than masquerading as a missing version. GitHub release
creation is likewise idempotent only for matching, non-draft notes. Never
overwrite an existing registry version, change a release tag, or weaken a
gate to recover. Correct unreleased source and use a new version when needed.

## Local preparation commands

`cargo xtask release` implements release logic in Rust rather than committed
shell scripts. Run it with `devenv shell -- rtk` from the workspace root:

| Subcommand | Purpose |
| --- | --- |
| `baseline` | Report whether a stable release tag exists; no network or credentials |
| `prepare [--tag vX.Y.Z]` | Require a clean checkout, dry-run Cargo, save archive/checksum/notes |
| `check --tag vX.Y.Z` | Repackage and compare a downloaded candidate before authentication |
| `registry-status` | Read-only crates.io lookup; missing or exact-match only |
| `verify-registry` | Wait for and validate the uploaded registry archive |
| `github-release` | CI finalization; writes GitHub release notes using job-scoped `GH_TOKEN` |

The first five commands need no publishing credential. The final command is
not a local diagnostic and must not be run during repository preparation.
