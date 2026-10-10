# Linux kernel tests

`vmtest` runs the `linux_live` executable against versioned upstream kernel
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
run in the guest. The wrapper passes devenv's resolved Rust version and vmtest
download URL as required build arguments to `tools/vmtest/Containerfile`; direct
builds must also supply `RUST_IMAGE` and `VMTEST_URL`. Enter the devenv shell
before invoking the wrapper. Cargo downloads and build outputs stay under
`tests/.cache/sakai-vmtest`.

The kernel versions and smoke selection are in `xtask/src/vmtest/kernel.rs`.
Fixtures and the staged executable are stored under
`tests/.cache/sakai-vmtest`. The executable metadata records the current
commit for diagnosis. The vmtest binary comes from the `devenv.yaml` input,
resolved in `devenv.lock`, on both platforms. QEMU comes from devenv on Linux
and the container's Debian packages on macOS. Container images and kernel
downloads have no manually maintained checksum pins. Kernel downloads use
a file lock and atomic rename; remove a cached image to download it again.
Kernel images (including cache hits) and the container's vmtest executable
are not independently checksum- or signature-verified. These paths trust
upstream releases and local cache contents; locks and atomic rename do not
establish artifact authenticity.

GitHub Actions builds the test executable and wrapper once, uploads them as a
short-lived artifact, and runs a separate job for each kernel. The matrix comes
from `cargo xtask kernel-matrix --profile smoke|full`, using the same Rust
definitions as local runs. The manual
workflow has a `vmtest_debug` option to retain the full console on failure.
See the [decision record](../../adr/e2e_live_linux_test.md) for the rationale.

## TODO

- Add an ARM64 `vmtest` executable and bootable ARM64 kernel
  fixtures so Apple Silicon can run a native, Rosetta-free kernel matrix.
- Add Fedora and Alpine guest smoke tests if distro-specific mount and
  delegation behavior becomes part of the support contract.
