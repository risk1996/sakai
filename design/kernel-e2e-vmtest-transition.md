# Linux kernel E2E transition — BoxLite to `danobi/vmtest`

This document is the implementation handoff for replacing BoxLite in Sakai's
live Linux-kernel tests. It is deliberately specific: implement the design in
small commits and benchmark before adding more machinery.

## Status

**Decision:** use the upstream [`danobi/vmtest`](https://github.com/danobi/vmtest)
CLI directly on GitHub-hosted Ubuntu runners. Do not use
`danobi/vmtest-action`, build kernels in CI, or compile Rust inside the VM.

The intended critical path is:

1. build the `linux_live` test executable once on the host;
2. fan out one GitHub Actions job per pinned kernel;
3. boot the prebuilt kernel with KVM;
4. run the already-built executable as guest root against a fresh cgroup v2
   mount; and
5. retain only a short console log on failure.

This preserves the useful part of the current test—running against real Linux
kernels—while removing BoxLite's OCI image, jailer, user-namespace workaround,
custom kernel rebuild, and nested guest Cargo build.

## Goals

- Exercise `sakai-core/tests/linux_live.rs` against multiple real upstream
  kernel series.
- Keep pull-request feedback fast while retaining broader scheduled coverage.
- Make every downloaded executable and kernel reproducible and checksum-pinned.
- Run the test in a writable, newly mounted cgroup v2 hierarchy as root.
- Keep the CI implementation small enough to debug from the QEMU console.
- Use the same entry point locally on macOS or x86-64 Linux and in CI.

## Non-goals

- Testing a distribution's init system, systemd delegation policy, OCI runtime,
  or container image.
- Building Linux in GitHub Actions.
- Hardware-accelerated x86-64 kernels on Apple Silicon. Local macOS runs the
  same Linux runner inside an amd64 container using QEMU emulation.
- Hiding kernel incompatibilities behind retries.
- Generalizing the harness into a VM-management abstraction.

## Why replace BoxLite

The current `kernels` job does more infrastructure work than the test needs:

- prepares BoxLite state and an OCI userspace;
- changes `/dev/kvm` permissions;
- disables Ubuntu's unprivileged-user-namespace AppArmor restriction;
- builds or adapts kernel artifacts for BoxLite's required drivers;
- starts a jailer and shim; and
- runs Cargo again in the guest.

The live test itself only needs a Linux kernel, a minimal init, the repository
filesystem, and a writable cgroup v2 mount. `vmtest` already supplies that
shape. For a kernel target, it boots QEMU, shares the host root through 9p,
shares the selected working directory at `/mnt/vmtest`, mounts a new cgroup v2
filesystem, and executes the requested command as root.[^vmtest-readme][^init]

`vmtest` also selects KVM automatically when the guest architecture matches the
host and `/dev/kvm` is available; otherwise QEMU falls back to emulation.[^qemu]
GitHub's x86-64 Ubuntu runners expose KVM, so the supported CI path remains
hardware accelerated.

## Chosen architecture

```text
                         build-live-test
                cargo test --no-run (host, once)
                              |
                    linux_live executable
                       GitHub artifact
                              |
          +-------------------+-------------------+
          |                   |                   |
     kernel 5.15         kernel 6.18       scheduled kernels
     download bzImage    download bzImage  6.1 / 6.6 / 6.12
     verify SHA-256      verify SHA-256    verify SHA-256
     vmtest + KVM        vmtest + KVM      vmtest + KVM
     run executable      run executable    run executable
```

There are two CI jobs:

1. **`build-live-test`** checks out the repository, restores the normal Cargo
   cache, enters the locked devenv, builds `linux_live` with `--no-run`, resolves
   the executable from Cargo's JSON output, and uploads that one executable.
2. **`kernels`** is a parallel matrix. Each entry downloads the executable,
   obtains one immutable kernel asset, verifies it, and runs it through
   `vmtest`.

Building once avoids repeating the expensive Rust dependency graph for every
kernel. Running the executable directly also avoids Cargo metadata scans over
9p, network access, registry state, and compiler memory inside the guest.

The downloaded artifact loses its executable mode under some artifact-client
versions, so the matrix job must explicitly run `chmod +x` before booting.

## Kernel matrix

Keep the current support policy:

| Event                              | Kernels                         | Purpose                                      |
| ---------------------------------- | ------------------------------- | -------------------------------------------- |
| Pull request, push, manual default | `5.15`, `6.18`                  | Oldest supported and newest pinned behavior. |
| Weekly schedule                    | `5.15`, `6.1`, `6.6`, `6.12`, `6.18` | Supported LTS-era compatibility sweep.       |

Use the prebuilt x86-64 fixtures published in the `test_assets` release of
`danobi/vmtest`. These are already the source of Sakai's fixture names; the
transition should restore direct use instead of rebuilding them for BoxLite.

| Series | Asset                         | SHA-256                                                           |
| ------ | ----------------------------- | ----------------------------------------------------------------- |
| 5.15   | `bzImage-v5.15-archlinux`     | `23879f21c7e3c7137902904fd89695fc9d8a938f1d2c1549dd1658443cb7a084` |
| 6.1    | `bzImage-v6.1-archlinux`      | `303b9010d92e4a9cf3930114f5c854edb2c5f7d1f9da2c3c29df7b1f7ab86a3c` |
| 6.6    | `bzImage-v6.6-archlinux`      | `3b47b1fefe02d49208da139bb1ad0363971ca5c02ab4ce89b9b8a52504b0fedf` |
| 6.12   | `bzImage-v6.12-archlinux`     | `a389c774c4bf035fbb7685be8493d128d8196c62862f750a8ef49b3b58428738` |
| 6.18   | `bzImage-v6.18-archlinux`     | `c2a05883f9556e73f5665d5979679163d80ff17754c4090e8d801df2a2e92551` |

The URL template is:

```text
https://github.com/danobi/vmtest/releases/download/test_assets/bzImage-v${series}-archlinux
```

Treat the table as data in one Rust module, not values duplicated in shell and
YAML. A new kernel requires an explicit source URL and SHA-256 review.

Do not silently change what a series name means. For example, `6.18` remains
the exact pinned fixture until a deliberate update changes its digest.

## Toolchain pinning

Pin the upstream `vmtest` executable in `devenv.nix` or an equivalent Nix
derivation:

- version: `v0.18.0` for the initial migration;
- platform: `vmtest-x86_64` on GitHub Actions;
- source URL and SHA-256: required in the Nix fetch expression; and
- QEMU plus `qemu-guest-agent`: provided by the same locked devenv.

Do not download `latest` at runtime. Do not use `cargo install vmtest` in CI:
that adds a second Rust build to every cold runner. The release binary makes
startup bounded by a small download or Nix cache realization.

Use the CLI directly rather than `danobi/vmtest-action`. The action wraps a
pinned CLI, installs Ubuntu packages in each matrix entry, and makes it awkward
to keep the command inside Sakai's locked environment. Direct invocation is
both smaller and easier to reproduce locally.[^action]

## Guest filesystem and safety

`vmtest` exposes the host root to the guest. Add `ro` to the guest kernel
arguments so that view is read-only. The repository is separately shared at
`/mnt/vmtest` and can remain writable for the test executable and its temporary
files.[^config]

The guest command must:

- set `SAKAI_VMTEST=1`;
- execute the uploaded test binary directly;
- pass `--nocapture` so failures reach the VM console;
- set a hard timeout outside the VM; and
- avoid network, Cargo, Nix, package-manager, and source-build commands.

The live test already creates and removes its own child cgroup. `vmtest`'s init
mounts a fresh cgroup v2 hierarchy at `/sys/fs/cgroup`, so the test should not
need a systemd delegation or host cgroup mutation.[^init]

Start with one vCPU and 1 GiB of RAM. The guest no longer compiles anything,
and the test is not CPU- or memory-intensive. Raise these values only if the
measured boot/test path is unstable; more virtual CPUs do not make this test
meaningfully faster.

## Repository changes

### 1. Keep a small Sakai wrapper

Retain `cargo xtask vmtest` as the stable developer interface, but replace its
BoxLite implementation. Its responsibilities should be limited to:

1. parse `--kernel` and `--profile`;
2. download a missing fixture to `tests/.cache/sakai-vmtest/kernels`;
3. verify SHA-256 before every first use and after a failed download;
4. locate or build the `linux_live` executable on the host;
5. invoke the upstream `vmtest` executable with the selected kernel and safe
   kernel arguments; and
6. return the guest command's exit status unchanged.

Use an atomic `*.partial` download followed by rename so a cancelled CI run
cannot turn a truncated image into a cache hit.

The wrapper should offer:

```console
devenv shell -- vmtest                         # 5.15 and 6.18
devenv shell -- vmtest --kernel 6.18
devenv shell -- vmtest --profile full         # all five fixtures
devenv shell -- vmtest --test-binary PATH     # CI artifact; never rebuild
```

The `--test-binary` form is the matrix fast path. If it is supplied, the
wrapper must not invoke Cargo.

### 2. Resolve the Cargo test executable robustly

For local runs and the build job, use:

```console
cargo test --locked --package sakai-core --test linux_live \
  --no-run --message-format=json-render-diagnostics
```

Parse Cargo's JSON messages in Rust and select the `compiler-artifact` whose
target name is `linux_live`, target kind includes `test`, and `executable` is
present. Do not glob `target/debug/deps/linux_live-*`; stale artifacts make a
glob nondeterministic.

Copy the resolved file to a stable artifact path such as:

```text
tests/.cache/sakai-vmtest/bin/linux_live
```

Also record the current commit and executable SHA-256 in a small metadata file.
This is diagnostic provenance, not a cache key.

### 3. Simplify `xtask`

Remove:

- the `boxlite` dependency and its transitive lockfile entries;
- OCI image selection and architecture probing;
- BoxLite machine, jailer, and state-directory management;
- custom kernel configuration/build logic added solely for BoxLite;
- BoxLite-specific log collection; and
- native-kernel profiles that do not correspond to a pinned matrix fixture.

Keep the fixture table, checksum verification, download locking, profile
selection, and concise command reporting. Split the upstream binary runner from
fixture acquisition so each part remains unit-testable.

### 4. Update `devenv`

Expose pinned `vmtest`, QEMU system binaries, and `qemu-guest-agent` inside
`devenv shell`. Preserve the existing `vmtest` script name as the project entry
point; give the upstream executable a non-conflicting internal path or invoke
its Nix-store path from `xtask`.

Verify these paths before merging:

```console
devenv shell -- vmtest --kernel 6.18
devenv shell -- command -v qemu-system-x86_64
devenv shell -- command -v qemu-ga
```

### 5. Replace the GitHub Actions kernel job

Delete both privileged BoxLite setup steps:

- `sudo chmod 666 /dev/kvm`; and
- `sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`.

Access to `/dev/kvm` should be verified, not globally broadened. Fail early
with an actionable message if the device is missing or unusable.

The workflow shape should be:

```yaml
build-live-test:
  # checkout + locked devenv + normal Cargo cache
  # cargo test --no-run --message-format=json-render-diagnostics
  # resolve tests/.cache/sakai-vmtest/bin/linux_live
  # upload linux_live with a short retention period

kernels:
  needs: build-live-test
  strategy:
    fail-fast: false
    matrix:
      kernel: # 5.15/6.18 normally; all five on schedule
  # checkout + lightweight locked runtime setup
  # download linux_live and chmod +x
  # restore/download the one kernel fixture and verify SHA-256
  # devenv shell -- vmtest --kernel "$kernel" --test-binary "$binary"
```

Use a 10-minute timeout for each kernel job initially. Once ten scheduled runs
show the p95 duration, set the timeout to at least twice that value, rounded up;
do not retain the current 30-minute allowance by inertia.

## Caching policy

Cache immutable inputs, not VM state.

| Data                         | Policy                                                                   |
| ---------------------------- | ------------------------------------------------------------------------ |
| Cargo registry/git           | Existing shared Cargo cache in `build-live-test`.                         |
| Cargo `target`               | Existing cache in `build-live-test`; key must not include kernel series. |
| `linux_live` executable      | GitHub artifact produced once per workflow run.                           |
| Kernel images               | Cache by series **and checksum**, or download the small immutable asset.  |
| `vmtest`/QEMU                | Nix/devenv cache keyed by lock files.                                     |
| VM disks, BoxLite state      | Delete; `vmtest` does not require them.                                   |
| Successful console logs      | Do not upload.                                                            |
| Failed console log           | Upload one bounded text artifact with short retention.                    |

Do not put `${{ github.sha }}` in the kernel cache key; kernel bytes do not
change when Sakai changes. Do not make five independent Cargo caches: the test
binary is kernel-independent.

Measure a direct release download against a GitHub cache restore before keeping
the kernel cache. At roughly tens of megabytes per fixture, cache negotiation
can cost as much as the download. Correctness must not depend on either path:
the checksum is authoritative.

## Diagnostics

Normal runs should show only:

- selected kernel and verified digest;
- whether KVM acceleration is active;
- QEMU boot/test duration; and
- test output and final exit status.

On failure, preserve the combined serial console from the beginning of boot
through shutdown. Cap or rotate it rather than searching a VM state tree for
many internal logs. The console already contains kernel panics, init failures,
the command line, and Rust test failures.

Add an opt-in manual-workflow debug mode that enables upstream `vmtest` debug
logging and retains the full console. Do not make verbose logging the default;
large logs slow both execution and diagnosis.

Do not retry a failing kernel automatically. A retry can hide a real race or
boot regression. If flaky runner infrastructure is demonstrated, retry only
the infrastructure boundary and clearly label both attempts.

## Migration sequence

Implement this as small, reversible commits:

1. Add the pinned `vmtest`/QEMU runtime to devenv and prove a one-off 6.18 boot.
2. Add the direct-executable runner behind a new `xtask` code path while the
   BoxLite path still exists.
3. Run `linux_live` locally on x86-64 Linux through both paths and compare
   assertions and cleanup.
4. Change the CI matrix to `vmtest`, retaining the current kernel policy.
5. Observe at least one pull-request run and one full scheduled run.
6. Remove BoxLite, its OCI/kernel-build code, privileged setup, cache entries,
   and documentation.
7. Rename the transitional code path to the ordinary `vmtest` entry point.

Do not keep two permanent backends. The overlap is only a migration aid and
should disappear in the same change series.

## Acceptance criteria

The transition is complete when all of the following are true:

- Pull requests run 5.15 and 6.18 in parallel and both execute
  `sakai-core/tests/linux_live.rs` against their reported guest kernel.
- The scheduled job runs all five pinned fixtures.
- A matrix job performs no Linux build, Cargo build, dependency download, Nix
  evaluation, or package installation inside the guest.
- The test executable is built once per workflow run, not once per kernel.
- Kernel and `vmtest` downloads are checksum-pinned.
- The guest host-root view is read-only and the cgroup hierarchy is fresh and
  writable.
- No BoxLite dependency, image, state directory, sysctl relaxation, or
  BoxLite-specific diagnostic step remains.
- A failed assertion returns a failing GitHub job and leaves a readable serial
  log; a successful job leaves no diagnostic artifact.
- Warm pull-request runs are faster than the BoxLite baseline, and the measured
  p95 kernel-job duration is documented in the final migration PR.

## Risks and mitigations

| Risk                                           | Mitigation                                                                                  |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Upstream release binary becomes unavailable   | Pin URL/hash; allow Nix or repository cache to retain the exact bytes.                       |
| A GitHub runner lacks usable KVM               | Preflight `/dev/kvm`; fail clearly instead of accepting slow TCG and timing out.             |
| Uploaded executable needs a Nix-store loader   | Use the same locked devenv in build and matrix jobs; verify with `ldd` during implementation. |
| 9p host-root access is broader than expected   | Pass kernel `ro`; keep writes under `/mnt/vmtest`; test this explicitly.                     |
| Fixture's config lacks a required controller   | Treat it as a fixture defect; replace and re-pin it rather than rebuilding on every run.     |
| Upstream CLI changes                           | Pin `v0.18.0`; wrap only the small argument surface Sakai uses.                              |
| Test binary artifact transfer costs too much   | Measure it; strip debug data only if backtraces remain sufficient and the saving is material. |

## Performance budget and measurement

Record these durations as GitHub step summaries for ten successful scheduled
runs:

1. environment realization;
2. host test build;
3. artifact upload/download;
4. kernel download/cache restore;
5. VM boot to command start;
6. test execution; and
7. guest shutdown.

Optimize the largest measured component. Expected order of effort:

1. remove all CI kernel compilation;
2. build the Rust test once;
3. keep kernel jobs parallel;
4. avoid work inside the guest;
5. keep immutable downloads pinned and cacheable; and
6. only then tune VM memory, logging, or boot arguments.

Do not add a custom initramfs, a self-hosted runner, or a new artifact service
without measurements showing the straightforward path cannot meet the desired
feedback time.

## References

[^vmtest-readme]: [`danobi/vmtest` README](https://github.com/danobi/vmtest/tree/v0.18.0), including kernel targets, host sharing, and command execution.
[^init]: [`vmtest` v0.18.0 init implementation](https://github.com/danobi/vmtest/blob/v0.18.0/src/init/init.sh.template), including the fresh `proc`, `sysfs`, device, and cgroup v2 mounts.
[^qemu]: [`vmtest` v0.18.0 QEMU implementation](https://github.com/danobi/vmtest/blob/v0.18.0/src/qemu.rs), including KVM selection and host CPU passthrough.
[^config]: [`vmtest` v0.18.0 configuration](https://github.com/danobi/vmtest/blob/v0.18.0/src/config.rs), including kernel arguments, CPU, memory, and shared-directory settings.
[^action]: [`danobi/vmtest-action`](https://github.com/danobi/vmtest-action), the optional GitHub Action wrapper intentionally not selected here.
