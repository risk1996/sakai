# Linux kernel tests

`vmtest` runs the `linux_live` executable against pinned upstream kernel
fixtures. On x86-64 Linux it runs directly with KVM. On macOS, the same
Linux runner runs inside an amd64 container, using QEMU emulation. The guest
shares the runner's root read-only and mounts a fresh writable cgroup v2
hierarchy.

```console
devenv shell -- vmtest                         # 5.15 and 6.18
devenv shell -- vmtest --kernel 6.18
devenv shell -- vmtest --profile full          # all five fixtures
devenv shell -- vmtest --test-binary PATH      # reuse a host-built executable
```

On macOS, start Apple Container, Podman, or Docker first. The wrapper uses the
first running engine in that order; set `SAKAI_CONTAINER_ENGINE` to choose one
explicitly. `--test-binary` is Linux-only because a macOS executable cannot
run in the guest. The base image digest and `vmtest` binary are pinned in
`tools/vmtest/Containerfile`, and Cargo downloads and build outputs stay under
`tests/.cache/sakai-vmtest`.

The kernel URLs and SHA-256 digests are in `xtask/src/vmtest/kernel.rs`.
Fixtures and the staged executable are stored under
`tests/.cache/sakai-vmtest`. The executable metadata records the current
commit and its digest for diagnosis. The pinned upstream `vmtest` v0.18.0
binary and QEMU come from the locked devenv on Linux and from the pinned
container image on macOS.

GitHub Actions builds the test executable and wrapper once, uploads them as a
short-lived artifact, and runs a separate job for each kernel. The manual
workflow has a `vmtest_debug` option to retain the full console on failure.
The transition design is in `design/kernel-e2e-vmtest-transition.md`.

## TODO

- Add a checksum-pinned ARM64 `vmtest` executable and bootable ARM64 kernel
  fixtures so Apple Silicon can run a native, Rosetta-free kernel matrix.
- Add Fedora and Alpine guest smoke tests if distro-specific mount and
  delegation behavior becomes part of the support contract.
