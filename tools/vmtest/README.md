# Linux kernel tests

The harness runs Sakai's live cgroup tests in
[`BoxLite`](https://github.com/boxlite-ai/boxlite) microVMs. BoxLite consumes
the pinned multi-architecture Rust OCI image directly, so no Docker daemon,
container build, or guest shell script is involved.

Run the quick matrix:

```console
devenv shell -- vmtest
```

Run every kernel compatible with the host architecture:

```console
devenv shell -- vmtest --profile full
```

Run only BoxLite's native kernel. On Apple Silicon this is an ARM64 VM and has
no Rosetta dependency:

```console
devenv shell -- vmtest --profile native
```

Select one or more named fixtures directly:

```console
devenv shell -- vmtest --kernel 6.18
```

The `vmtest` command forwards its arguments to `cargo xtask vmtest`; run
`cargo xtask vmtest --help` for the complete interface.

The native BoxLite Rust crate is pinned in `xtask/Cargo.toml`, with its
transitive dependencies recorded in `Cargo.lock`. The kernel matrix and its
checksums live in `xtask/src/vmtest/kernel.rs`. Downloaded kernels, BoxLite
state, and guest Cargo artifacts are retained under `tests/.cache/sakai-vmtest`.

The native entry deliberately uses BoxLite's bundled kernel. The x86-64
entries cover the supported kernel series with custom kernels. The Rust live
test creates and cleans up its own cgroup, while `procfs` discovers the active
cgroup2 mount rather than assuming `/sys/fs/cgroup`.

## TODO

- Add pinned ARM64 custom-kernel fixtures when a maintained source publishes
  kernels with the virtio configuration required by BoxLite.
- Add Fedora and Alpine guest smoke tests when Sakai begins testing distro
  init, delegation, and namespace behavior in addition to the kernel ABI.
