# crates.io pre-publication checklist for `sakai`

Research checked on 2026-09-30. Use this before the **first** production
publication of `sakai`. Checked boxes record completed preparation; this is
a release gate, not a claim that the crate is ready. The scope and priority of
unfinished interfaces remain in [`TODO.md`](../TODO.md), and the release direction is in
[project-direction.md](project-direction.md).

Cargo's [publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
warns that a published version cannot be overwritten. Review the packaged
crate, not only the working tree. Commands below name `sakai` explicitly
because the workspace also contains `xtask` and `sakai-python`.

Verification baseline: commit `ec70f57c04bb701fad382fb822a4d607b87838be`,
reviewed on 2026-10-11. Dated results below describe that source commit,
not a final release candidate. README/package metadata and release automation
were deferred at that baseline. The current implementation and bootstrap
sequence are in [releases.md](releases.md); account setup alone is not an upload.
Repeat artifact checks after changing packaged files or metadata,
and repeat the release-commit checks before upload.

CI remains Linux-only. Existing Linux 5.15/6.18 smoke coverage is sufficient
for this preparation; an additional full/intermediate-kernel matrix run is
not a release requirement. These decisions do not change the existing CI jobs.

API-boundary follow-up on 2026-10-11: `devenv --profile python test` passed
Rust formatting/Clippy, 43 local workspace tests, seven public doctests, and
Python lint/format/type checks. Rust 1.88 passed all seven doctests and the
all-targets/all-features Linux cross-check. Linux Rust 1.99 / CPython 3.11
passed 52 crate unit/fixture tests, 15 binding tests (including exception paths
and P2/P3 conversions), and seven doctests. These are compiler/test checks of
the API changes, not a new kernel-matrix run. The artifact results and digest
below still identify the earlier baseline; package verification must be
repeated for the changed source.

## 1. Settle the crate contract and metadata

- [x] Review the currently implemented API: public types, Linux-only handle gating,
  parser availability on other platforms, missing-file behavior, read-only
  guarantees, and non-atomic fresh reads. On 2026-10-11, CPU, memory (including
  swap/zswap), process-number, core, and I/O readers were reviewed. Handles
  and borrowed reader views are Linux-gated; snapshot types and `FromStr`
  implementations remain portable. Reads open fresh `RDONLY` descriptors,
  never enable controllers or write PSI triggers, and report absent files as
  `Error::FileMissing` with their path. Optional counters remain distinct from
  zero; `CpuMax::cpu_count()` describes only this cgroup's quota/period.
  Existing tests cover these contracts, including missing/older forms and
  descriptor-relative reads. No API changes were made by this review.
  [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html),
  [crate root](../sakai/src/lib.rs),
  [handle implementation](../sakai/src/cgroup/v2/handle.rs),
  [file reads](../sakai/src/cgroup/v2/file.rs).
- [x] Settle which implemented interfaces belong in the first release.
  The implemented P0–P3 interfaces, including process/core and I/O readers,
  remain in the Rust and Python release surface. On 2026-10-11, the project
  owner confirmed this scope and `TODO.md` was reconciled; P4 and later work
  remains deferred.
- [ ] Confirm that `sakai` is available on crates.io immediately before
  release and that the intended crates.io account controls the name. Crate
  names are allocated first come, first served.
  [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).
- [x] Add a real license file and set `license` to its SPDX expression, or use
  `license-file` for a nonstandard license. Inspect the packaged archive to
  ensure the license text is present. MIT is set through workspace metadata;
  the crate-local [LICENSE](../sakai/LICENSE) symlinks to the root
  [LICENSE](../LICENSE). On 2026-10-10, an isolated working-tree snapshot was
  packaged and built with `cargo package -p sakai --locked --offline`;
  the archive's regular license file, exact text, and normalized
  `license = "MIT"` were verified.
  The descriptive metadata is covered by the item below.
  [Cargo manifest reference](https://doc.rust-lang.org/cargo/reference/manifest.html).
- [x] Fill in accurate `[package]` metadata: concise `description`, source
  `repository`, crate-local `readme`, and useful `keywords`/`categories` if
  applicable. Set `documentation` only if using a site other than docs.rs;
  set `homepage` only if it is a distinct project site. Avoid a workspace-root
  README that the packaged crate cannot render correctly by itself.
  Added a crate-local README, description, repository URL, keywords, and
  categories on 2026-10-11. Final archive verification remains a separate gate.
  [Cargo manifest reference](https://doc.rust-lang.org/cargo/reference/manifest.html),
  [Rust API Guidelines on metadata](https://rust-lang.github.io/api-guidelines/documentation.html).
- [x] Choose and document the minimum supported Rust version (MSRV), then set
  `rust-version` and test it. Edition 2024 requires at least Rust 1.85; a newer
  language feature or dependency may raise the actual floor. State whether
  the policy is fixed or tracks stable Rust.
  `sakai` inherits `rust-version = "1.88.0"` from workspace metadata;
  the [README](../README.md) documents a fixed Rust 1.88 minimum. The
  existing `nutype` 0.8 dependency uses let chains, stabilized in
  [Rust 1.88](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/).
  On 2026-10-10, Rust 1.88.0 passed all 32 crate unit tests and both doctests
  on macOS, plus an all-targets, all-features cross-check for
  `x86_64-unknown-linux-gnu`, using the locked dependencies. `devenv test`
  also passed. The existing Ubuntu CI checks job now compiles and tests all crate
  targets and features and runs doctests on Rust 1.88.0; it runs repository
  tests and lints with the development toolchain. The baseline's Ubuntu
  [checks job](https://github.com/risk1996/sakai/actions/runs/38108925883/job/114380216737)
  passed, including all three MSRV steps. No additional MSRV work is required
  for this preparation; rerun the existing CI checks on the release commit.
  [Rust 1.85 release](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html),
  [Cargo rust-version guidance](https://doc.rust-lang.org/cargo/reference/rust-version.html).
- [x] Review the version number and SemVer implications of public enums,
  trait implementations, types, and error behavior. The current workspace and
  normalized package both use `0.1.0`; there are no local release tags or a
  previous published baseline selected for a compatibility comparison.
  For the initial `0.1` line, incompatible API changes require `0.2.0`;
  compatible additions/fixes can remain in `0.1.x`. The follow-up marks
  growable error, controller, and memory-field enums `#[non_exhaustive]`;
  downstream matches require a fallback. Closed `MaxOr`, `CpuWeight`,
  `CgroupType`, and `FieldKind` alternatives remain exhaustive. The scalar
  module and CPU parser field enum are private; interface-specific snapshot
  aliases, their methods and traits, public `uom` aliases, generated `nutype`
  methods, and parser errors remain compatibility commitments. Keep
  missing-file, optional-field, unit, and fresh-read behavior compatible
  within that line. Public doctests compile consumer use of snapshot aliases
  and reject storage/parser imports, mismatched interfaces, and exhaustive
  downstream error matches. Python preserves its existing typed error
  handling and maps future core variants to `SakaiError`. Stable Rust cannot
  require an exhaustive external match on a non-exhaustive enum; internal
  exhaustive matches remain compiler-checked.
  [Cargo SemVer reference](https://doc.rust-lang.org/cargo/reference/semver.html),
  [scalar API](../sakai/src/scalar.rs), [errors](../sakai/src/error.rs).
- [ ] Align the final version, release notes/changelog, and release tag with
  `Cargo.toml` after settling release scope.
  [publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).
- [x] Make the crate-local README and rustdoc examples compile against the
  released API. Show Linux gating explicitly: `Cgroup` is Linux-only, while
  parsers and snapshot values are portable. Explain units, `MaxOr::Max`,
  controller-disabled files, and `CpuMax::cpu_count()`'s limited meaning.
  The crate README is included in the workspace doctests. On 2026-10-11,
  `devenv test` passed its examples on macOS, alongside 49 workspace tests,
  formatting, and Clippy. Release CI covers the Linux-gated example too.
  [Rust API Guidelines on documentation](https://rust-lang.github.io/api-guidelines/documentation.html),
  [current crate root](../sakai/src/lib.rs).

## 2. Validate what Cargo will actually publish

- [x] Run the repository checks with `devenv test`. On 2026-10-11 the clean
  baseline worktree passed formatting, Clippy, all 43 macOS workspace nextest
  tests, and three crate doctests using Rust 1.99.0. Local macOS execution
  verifies portable parsers; it does not execute Linux handle/live tests.
  The baseline's Ubuntu checks job also passed Rust/Python gates and MSRV
  checks. CI stays Linux-only; no macOS runner is required.
- [x] Confirm existing Linux live and smoke-kernel coverage on the baseline.
  The [Linux 5.15 job](https://github.com/risk1996/sakai/actions/runs/38108925883/job/114380505049)
  and [Linux 6.18 job](https://github.com/risk1996/sakai/actions/runs/38108925883/job/114380505025)
  passed. No additional matrix run was performed or is required for this
  preparation. Continue using the existing smoke coverage on the release
  commit. [Repository test guidance](../README.md).
- [x] Check dependency advisories and review licenses of runtime and build
  dependencies; resolve material findings or record a concrete rationale.
  Confirmed the existing [security job](https://github.com/risk1996/sakai/actions/runs/38108925883/job/114380216632)
  passed on the baseline on 2026-10-11: `check:audit` scanned 216 locked
  dependencies against 1,296 RustSec advisories, and `check:deny` passed the
  advisory/license/source/bans policy in [deny.toml](../deny.toml), including
  development dependencies and all targets. Its only reported warning was
  duplicate `syn` 2.0.119/3.0.7, permitted by the existing multiple-version
  warning policy; this is not an advisory or license failure. No new security
  tooling or redundant local audit was added. Reuse
  `devenv tasks run check:security` and the existing CI job for later commits.
  `cargo audit` checks the lockfile against RustSec advisories, but a clean
  report does not prove every dependency is safe.
  [RustSec cargo-audit](https://github.com/rustsec/rustsec/blob/main/cargo-audit/README.md).
- [x] Use `devenv shell -- rtk cargo package -p sakai --list --locked`
  and inspect the baseline archive's complete file list and size. Its 72
  regular files include Cargo's generated manifests/lockfile/VCS information,
  the MIT license, all Rust sources, parser/procfs fixtures, and
  `tests/linux_live.rs`. There are no packaged Python bindings, `xtask`, CI
  files, caches, build outputs, or credential files. A targeted scan found
  no embedded checkout paths or common credential patterns; this is not an
  exhaustive secret audit. The archive is 47,809 bytes (46.7 KiB).
  [Cargo package](https://doc.rust-lang.org/cargo/commands/cargo-package.html).
- [ ] Repeat file-list/archive inspection after adding final metadata and
  the crate-local README. The baseline has no README and normalizes to
  `readme = false`; the root README is not included automatically. Confirm
  the eventual README and all required files are present before publication.
- [x] Run `devenv shell -- rtk cargo package -p sakai --locked` and
  `devenv shell -- rtk cargo publish -p sakai --dry-run --locked` from a
  clean baseline commit. Both passed on 2026-10-11; the dry run explicitly
  aborted the upload. Cargo warned about absent descriptive metadata, which
  remains deferred above. No warning suppression or verification bypass was
  used. Do not use `--allow-dirty`, `--no-verify`, or
  `--no-metadata` to get past warnings. Cargo extracts and builds the package
  during verification; `--dry-run` performs checks without uploading.
  [Cargo package](https://doc.rust-lang.org/cargo/commands/cargo-package.html),
  [Cargo publish](https://doc.rust-lang.org/cargo/commands/cargo-publish.html).
- [x] Inspect `target/package/sakai-<version>.crate` itself, including
  Cargo's normalized manifest and `Cargo.lock`. Build and test from an
  extracted archive or fresh external consumer project with no access to the
  workspace. This catches accidental reliance on files, workspace members,
  patches, or paths that disappear when Cargo packages the crate.
  The baseline normalized manifest contains `0.1.0`, Rust `1.88.0`, MIT,
  explicit library/live-test targets, and registry dependency declarations;
  no workspace inheritance, patches, or path dependencies remain. The
  packaged lockfile contains only the crate and its dependency graph. Its
  license is a regular file identical to the root license. After extraction
  outside the checkout, macOS tests passed; an isolated Linux container with
  no checkout mounted passed all 52 library unit/fixture tests and checked
  all targets/features with networking disabled. Additional live/kernel runs
  were deliberately not repeated.
  [Cargo package contents](https://doc.rust-lang.org/cargo/commands/cargo-package.html).
- [x] Confirm all regular dependencies are registry-resolvable; any future
  local `path` dependency also needs a published version requirement. Check
  the dependency ranges in a fresh consumer resolution, since a library's
  packaged lockfile does not pin dependencies for ordinary downstream builds.
  Cargo metadata confirms all six regular dependencies (including Linux-only
  `procfs`/`rustix`) use crates.io requirements. A fresh external consumer of
  the extracted crate resolved online with its own lockfile and compiled on
  macOS and isolated Linux. Its graph contains only that consumer, the extracted `sakai`, and
  36 registry packages; no Python/xtask workspace member participates.
  This checks current compatible resolution, not every historical version
  admitted by each dependency range.
  [Cargo dependency rules](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html),
  [Cargo package contents](https://doc.rust-lang.org/cargo/commands/cargo-package.html).
- [x] Build rustdoc and run doctests from the packaged crate. Check that
  docs.rs's default Linux target can document the public API without files
  outside the archive or network access; add docs.rs metadata only if a
  specific build setting is needed.
  On the baseline, isolated aarch64 Linux with Rust 1.99.0 passed rustdoc
  with `RUSTDOCFLAGS=-Dwarnings`, all three doctests, and all-target checks
  offline. Dependencies were fetched before disabling container networking.
  The docs.rs default `x86_64-unknown-linux-gnu` documentation build was also
  checked offline using the existing Rust 1.88.0 target installation.
  No docs.rs-specific metadata is needed for the current build.
  [docs.rs builds](https://docs.rs/about/builds),
  [docs.rs metadata](https://docs.rs/about/metadata).

## 3. Prepare the first upload and later automation

Deferred until release preparation: no account, token, tag, workflow, or
registry configuration was changed during the baseline verification.

- [ ] Ensure the crates.io account has a verified email and access to the
  intended crate name. For the **first** release, prepare a personal API
  token for a manual `devenv shell -- rtk cargo publish -p sakai --locked`;
  handle it as a secret and avoid committing it or storing it in CI. Cargo's
  credential provider can use an OS keychain; its plain token provider stores credentials
  unencrypted on disk.
  [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html),
  [registry authentication](https://doc.rust-lang.org/cargo/reference/registry-authentication.html).
- [x] Prepare the tag-gated release workflow and protect its tag pattern, but
  leave the first crate publication manual. The crates.io team says a crate
  must have an initial release before its Trusted Publisher can be configured.
  After that first release, register the exact repository/workflow identity
  and use the official auth action and job-scoped `id-token: write` for later
  releases. Keep publish credentials out of pull-request jobs.
  Added `release.yml` with existing reusable CI, candidate/checksum gates,
  protected `crates-io` publishing, and step-scoped SecretSpec env credentials.
  The owner confirmed the tag ruleset and environment protection setup.
  No production upload or OIDC token exchange has been performed; trusted
  publisher registration and first real upload verification remain pending.
  [crates.io Trusted Publishing announcement](https://blog.rust-lang.org/2025/07/11/crates-io-development-update-2025-07/),
  [Rust Forge workflow guidance](https://forge.rust-lang.org/infra/docs/trusted-publishing.html).
- [ ] Review the exact commit, version, release notes, and tag before the
  manual upload. Run the final dry run on that commit; publish only that
  reviewed tree. Keep the `.crate` file and its digest for comparison with
  the registry release. A yank can prevent new resolutions but cannot repair
  or erase an already published archive.
  [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).

## Final go/no-go

Publish to crates.io only after every applicable item above passes, the crate
name and license are settled, the clean packaged archive has been inspected,
and the manual first-release credential is ready. If a check changes the
contents or manifest, repeat package inspection and the dry run. This checklist
ends before the upload; checking the published crate, docs.rs page, and registry
ownership belongs in the release runbook.

## Repeat the baseline verification

Use the existing Cargo, devenv, tar, and container tools. Run package commands
from a clean change/release worktree. These commands do not upload anything;
dependency resolution and image/dependency downloads do use the network.
An extracted package test is useful beyond Cargo's automatic build verification
because it executes fixture tests and doctests. A new consumer tests resolution
without inheriting the library's lockfile.

```sh
devenv test
devenv shell -- rtk cargo package -p sakai --list --locked
devenv shell -- rtk cargo package -p sakai --locked
devenv shell -- rtk cargo publish -p sakai --dry-run --locked

crate_version="$(devenv shell -- rtk cargo metadata --no-deps --format-version 1 | rtk jq -r '.packages[] | select(.name == "sakai") | .version')"
crate_archive="target/package/sakai-${crate_version}.crate"
verification_dir="$(rtk mktemp -d)"
rtk tar -tzvf "$crate_archive"
rtk shasum -a 256 "$crate_archive"
rtk tar -xzf "$crate_archive" -C "$verification_dir"
crate_manifest="$verification_dir/sakai-${crate_version}/Cargo.toml"
rtk cmp LICENSE "$verification_dir/sakai-${crate_version}/LICENSE"
devenv shell -- rtk cargo metadata --manifest-path "$crate_manifest" --format-version 1 --locked
devenv shell -- rtk cargo test --manifest-path "$crate_manifest" --lib --all-features --locked --offline
devenv shell -- rtk cargo test --manifest-path "$crate_manifest" --doc --all-features --locked --offline

devenv shell -- rtk cargo new --lib --vcs none "$verification_dir/consumer"
devenv shell -- rtk cargo add --manifest-path "$verification_dir/consumer/Cargo.toml" sakai --path "$verification_dir/sakai-${crate_version}"
devenv shell -- rtk cargo check --manifest-path "$verification_dir/consumer/Cargo.toml" --locked
devenv shell -- rtk cargo metadata --manifest-path "$verification_dir/consumer/Cargo.toml" --format-version 1 --locked
```

For isolated Linux checks, use a running container engine. Derive its Rust
image version from the resolved development compiler. Only the temporary
verification directory is mounted; the source checkout is unavailable.
Fetch both locked resolutions when their generated lockfiles differ, so the
offline consumer check does not rely on the crate's dependency cache alone.

```sh
rust_version="$(devenv shell -- rtk rustc --version | rtk awk '{print $2}')"
rtk docker run --rm \
  --mount "type=bind,src=$verification_dir,dst=/verification" \
  --workdir "/verification/sakai-${crate_version}" \
  --env CARGO_HOME=/verification/linux-cargo \
  --env CARGO_TARGET_DIR=/verification/linux-target \
  "rust:${rust_version}-bookworm" sh -c \
  'cargo fetch --locked &&
  if ! cmp -s Cargo.lock /verification/consumer/Cargo.lock; then
    cargo fetch --manifest-path /verification/consumer/Cargo.toml --locked
  fi'
rtk docker run --rm --network none \
  --mount "type=bind,src=$verification_dir,dst=/verification" \
  --workdir "/verification/sakai-${crate_version}" \
  --env CARGO_HOME=/verification/linux-cargo \
  --env CARGO_TARGET_DIR=/verification/linux-target \
  --env RUSTDOCFLAGS=-Dwarnings \
  "rust:${rust_version}-bookworm" sh -c \
  'cargo test --lib --all-features --locked --offline && cargo check --all-targets --all-features --locked --offline && cargo doc --no-deps --all-features --locked --offline && cargo test --doc --all-features --locked --offline && cargo check --manifest-path /verification/consumer/Cargo.toml --locked --offline'
```

On an x86-64 Linux host this also uses docs.rs's default target. For a separate
cross-documentation check, use `cargo doc --target x86_64-unknown-linux-gnu`
with that target installed. When selecting a rustup compiler inside devenv,
set `RUSTC` and `RUSTDOC` to `rustup which --toolchain <version> rustc|rustdoc`,
as the existing MSRV CI does; `rustup run` alone does not override devenv's
compiler environment. Local macOS checks remain optional, outside CI.

Baseline archive SHA-256:
`24906d305f5e7a6cfaee0b0e75c7697827758b3230e780821e5ca2cda7a34c0d`.
This identifies the inspected baseline bytes, not the eventual release archive.
