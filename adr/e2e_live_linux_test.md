---
status: accepted
date: 2026-09-29
---

# Test the live Linux cgroup interface with vmtest

## Context and Problem Statement

Sakai reads and writes the Linux cgroup v2 interface. Parser fixtures and
container-only tests cannot establish that those operations work across kernel
versions: a container uses its host VM's kernel. The test must run from macOS
as well as Linux, remain practical in GitHub Actions, and avoid maintaining a
VM runtime of our own.

## Decision Drivers

- Test real, pinned Linux kernels and a writable cgroup v2 hierarchy.
- Keep the guest small: no Cargo build or package installation inside it.
- Share one developer entry point with CI and keep the kernel matrix bounded.
- Pin and verify `vmtest` and kernel downloads; record the test executable's
  build provenance.

## Considered Options

- BoxLite microVMs with a guest OCI image and custom kernel adaptation.
- The upstream `danobi/vmtest` CLI, called by a small Rust `xtask` wrapper.
- `danobi/vmtest-action` as the CI entry point.

## Decision Outcome

Use the upstream `vmtest` CLI through `cargo xtask vmtest`. BoxLite required
additional OCI, jailer, namespace, and kernel-adaptation work that the cgroup
test does not need. The GitHub Action adds another wrapper and package-install
path; invoking the pinned CLI directly keeps local and CI behavior aligned.

`xtask` builds `sakai`'s `linux_live` test executable on the Linux runner
(inside a container on macOS), identifies it from Cargo's JSON artifact
messages, and runs that executable in each guest. The guest gets the Linux
runner's root read-only, a shared executable at `/mnt/vmtest`, and a fresh
writable cgroup v2 mount. The Rust live test creates and cleans up its own
cgroup fixture; it also exercises cgroup discovery rather than assuming one
mount point. No repository guest shell script or guest Cargo build is needed.

The x86-64 kernel fixtures are `5.15`, `6.1`, `6.6`, `6.12`, and `6.18`.
`xtask/src/vmtest/kernel.rs` owns their URLs, SHA-256 digests, and smoke/full
selection. Downloads are locked, checksum-verified, and staged before rename
under `tests/.cache/sakai-vmtest`. The upstream `vmtest` version is pinned to
`v0.18.0` in devenv and the macOS container definition.

On x86-64 Linux, the wrapper runs `vmtest` directly, using KVM when available.
On macOS, it builds an amd64 image from `tools/vmtest/Containerfile` and runs
the same Linux wrapper inside Apple Container, Podman, or Docker. This path uses
QEMU emulation and currently requires x86 translation on Apple Silicon.

GitHub Actions builds the Linux test executable and wrapper once, then runs one
parallel job per kernel with a preflight KVM check. Pushes to `main` and pull
requests run `5.15` and `6.18`; the weekly schedule runs all five. Failure
consoles are retained as bounded artifacts. The manual debug option retains
the full console, including on success.

### Consequences

- The matrix tests distinct kernel versions without building Linux or Rust in
  each guest; CI kernel jobs reuse one build artifact.
- Local macOS runs work, but x86 emulation is slower and is not a Rosetta-free
  ARM64 validation path.
- The matrix tests the kernel ABI, not Fedora or Alpine init, mount, or
  delegation policies. Native ARM64 kernel fixtures and distro smoke tests
  remain separate follow-up work.

### Confirmation

The five-kernel matrix passed on an Apple Silicon Mac through Docker on
2026-09-27; the warm-cache run took 2 minutes 59 seconds. The corresponding
repository checks (`devenv test`) passed. CI is configured for KVM-backed
parallel execution, but this local run does not establish CI runtime or p95
performance.

## References

- [vmtest](https://github.com/danobi/vmtest) and its [configuration](https://github.com/danobi/vmtest/blob/v0.18.0/docs/config.md)
- [Local usage and follow-up work](../tools/vmtest/README.md)
- [Kernel fixtures](../xtask/src/vmtest/kernel.rs) and [runner](../xtask/src/vmtest/mod.rs)
