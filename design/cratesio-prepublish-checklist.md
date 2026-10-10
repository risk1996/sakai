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

## 1. Settle the crate contract and metadata

- [ ] Decide which currently implemented cgroup v2 interfaces belong in the
  first published API. Check that the public types, Linux-only handle gating,
  parser availability on other platforms, missing-file behavior, read-only
  guarantees, and non-atomic fresh reads are stable enough to document.
  [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html),
  [current README](../README.md).
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
  `description` is still absent and belongs to the metadata item below.
  [Cargo manifest reference](https://doc.rust-lang.org/cargo/reference/manifest.html).
- [ ] Fill in accurate `[package]` metadata: concise `description`, source
  `repository`, crate-local `readme`, and useful `keywords`/`categories` if
  applicable. Set `documentation` only if using a site other than docs.rs;
  set `homepage` only if it is a distinct project site. Avoid a workspace-root
  README that the packaged crate cannot render correctly by itself.
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
  also passed. The existing Ubuntu CI checks job now compiles all crate
  targets and features on Rust 1.88.0; it runs repository tests and lints
  with the development toolchain. Execution of CI and the broader release
  checks remain gates below, including macOS verification before publication.
  [Rust 1.85 release](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html),
  [Cargo rust-version guidance](https://doc.rust-lang.org/cargo/reference/rust-version.html).
- [ ] Review the version number and SemVer promise, including changes to
  public enums, trait implementations, types, and error behavior. Keep the
  release notes/changelog and version tag aligned with `Cargo.toml`.
  [Cargo SemVer reference](https://doc.rust-lang.org/cargo/reference/semver.html),
  [publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).
- [ ] Make the crate-local README and rustdoc examples compile against the
  released API. Show Linux gating explicitly: `Cgroup` is Linux-only, while
  parsers and snapshot values are portable. Explain units, `MaxOr::Max`,
  controller-disabled files, and `CpuMax::cpu_count()`'s limited meaning.
  [Rust API Guidelines on documentation](https://rust-lang.github.io/api-guidelines/documentation.html),
  [current crate root](../sakai/src/lib.rs).

## 2. Validate what Cargo will actually publish

- [ ] Run the repository checks with `devenv test`; confirm format, Clippy,
  unit/integration tests, and doctests pass on the release commit. Add the
  chosen MSRV to CI and test the crate on both Linux and macOS, since the
  published API promises portable parsers. Do not equate a macOS compile
  with support for Linux-only cgroup handles.
- [ ] Run Linux live tests and the supported kernel VM matrix, checking
  optional files and older-kernel forms without requiring host cgroup writes.
  Resolve any mismatch between documented and observed kernel behavior before
  publication. [Repository test guidance](../README.md).
- [ ] Check dependency advisories and review licenses of runtime and build
  dependencies; resolve material findings or record a concrete rationale.
  `cargo audit` checks the lockfile against RustSec advisories, but a clean
  report does not prove every dependency is safe.
  [RustSec cargo-audit](https://github.com/rustsec/rustsec/blob/main/cargo-audit/README.md).
- [ ] Use `devenv shell -- rtk cargo package -p sakai --list --locked`
  and inspect every included file. Include Rust sources, tests/fixtures used by
  the package, README, license, and required manifests; exclude unrelated
  workspace tools, generated output, caches, and secrets. Check the final
  `.crate` size. [Cargo package](https://doc.rust-lang.org/cargo/commands/cargo-package.html).
- [ ] Run `devenv shell -- rtk cargo package -p sakai --locked` and
  `devenv shell -- rtk cargo publish -p sakai --dry-run --locked` from a
  clean release commit. Do not use `--allow-dirty`, `--no-verify`, or
  `--no-metadata` to get past warnings. Cargo extracts and builds the package
  during verification; `--dry-run` performs checks without uploading.
  [Cargo package](https://doc.rust-lang.org/cargo/commands/cargo-package.html),
  [Cargo publish](https://doc.rust-lang.org/cargo/commands/cargo-publish.html).
- [ ] Inspect `target/package/sakai-<version>.crate` itself, including
  Cargo's normalized manifest and `Cargo.lock`. Build and test from an
  extracted archive or fresh external consumer project with no access to the
  workspace. This catches accidental reliance on files, workspace members,
  patches, or paths that disappear when Cargo packages the crate.
  [Cargo package contents](https://doc.rust-lang.org/cargo/commands/cargo-package.html).
- [ ] Confirm all regular dependencies are registry-resolvable; any future
  local `path` dependency also needs a published version requirement. Check
  the dependency ranges in a fresh consumer resolution, since a library's
  packaged lockfile does not pin dependencies for ordinary downstream builds.
  [Cargo dependency rules](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html),
  [Cargo package contents](https://doc.rust-lang.org/cargo/commands/cargo-package.html).
- [ ] Build rustdoc and run doctests from the packaged crate. Check that
  docs.rs's default Linux target can document the public API without files
  outside the archive or network access; add docs.rs metadata only if a
  specific build setting is needed.
  [docs.rs builds](https://docs.rs/about/builds),
  [docs.rs metadata](https://docs.rs/about/metadata).

## 3. Prepare the first upload and later automation

- [ ] Ensure the crates.io account has a verified email and access to the
  intended crate name. For the **first** release, prepare a personal API
  token for a manual `devenv shell -- rtk cargo publish -p sakai --locked`;
  handle it as a secret and avoid committing it or storing it in CI. Cargo's
  credential provider can use an OS keychain; its plain token provider stores credentials
  unencrypted on disk.
  [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html),
  [registry authentication](https://doc.rust-lang.org/cargo/reference/registry-authentication.html).
- [ ] Prepare the tag-gated release workflow and protect its tag pattern, but
  leave the first crate publication manual. The crates.io team says a crate
  must have an initial release before its Trusted Publisher can be configured.
  After that first release, register the exact repository/workflow identity
  and use the official auth action and job-scoped `id-token: write` for later
  releases. Keep publish credentials out of pull-request jobs.
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
