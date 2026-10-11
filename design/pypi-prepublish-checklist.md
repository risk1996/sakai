# PyPI pre-publication checklist

Research checked on 2026-09-30. Use this for the implemented
[Python bindings](../sakai-python/README.md) **before uploading a
release to production PyPI**. Checked boxes record completed preparation;
unchecked items remain release gates. A successful local import is not the
release gate; test the actual archives that will be uploaded.

First release target: Linux CPython, Python 3.11 minimum,
`abi3-py311`, package import `sakai`, built with Maturin. Do not silently add
macOS/Windows wheels or claim free-threaded CPython support while the Linux-only
handle API and ABI plan remain as documented. Sources for these package-specific
choices are the package metadata and current Rust code; the external best-practice
sources are linked below.

Verification baseline (2026-10-11): the Linux wheel/sdist candidates below
were built from clean commit `ae898a12a72414cda5855036d17a304050ec8768`,
not a final release candidate. Checked preparation items describe that baseline;
repeat archive checks after changing packaged files or metadata. README/package
information, registry uploads (including TestPyPI), credentials, publishers,
and release automation remain deferred. No macOS CI or additional kernel matrix
is required; existing Linux 5.15/6.18 smoke coverage is sufficient here.

The proposed wheel matrix is regular CPython 3.11–3.15 on Linux x86_64 and
aarch64, glibc ≥2.17 (`manylinux2014`). Both architectures were verified locally,
but the release support promise still needs owner confirmation. Musl, other
architectures, macOS, Windows, PyPy, and free-threaded CPython are outside this
candidate wheel matrix. Successful tests on newer glibc hosts plus native audits
are not execution tests on glibc 2.17 itself.

## 1. Settle the public release contract

- [ ] Confirm the distribution name with PyPI immediately before release, its
  normalized spelling, and that the project owner controls it. Align the name
  in `pyproject.toml`, documentation, release workflow, and PyPI Trusted
  Publisher configuration. A pending publisher does **not** reserve the name.
  [PyPI project creation](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/),
  [name rules](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/).
- [x] Choose and add a license for the Rust and Python code; include the actual
  license file(s) in both wheel and sdist. Record a valid SPDX expression and
  `license-files` in Python metadata; make `sakai` metadata consistent.
  MIT covers both languages through the root [LICENSE](../LICENSE),
  package-local symlinks, Cargo workspace metadata, and Python's
  `license = "MIT"` / `license-files = ["LICENSE"]`. On 2026-10-10, Maturin
  1.15.0 built a wheel and sdist from an isolated working-tree snapshot;
  both archives had `License-Expression: MIT`, `License-File: LICENSE`, and
  the exact license text in regular files (the symlinks were dereferenced).
  The macOS wheel was only a local packaging check;
  on 2026-10-11, the two Linux wheels and sdist below also had identical MIT
  metadata and exact license text in regular archive files. The sdist includes
  the root, Rust-crate, and binding-crate license files.
  [Python metadata specification](https://packaging.python.org/en/latest/specifications/pyproject-toml/).
- [x] Review the initial Python surface: class and method names,
  unit suffixes, `MaxOr.is_max` and its guarded `.value`, optional older-kernel
  fields, exception types, and the no-atomic-snapshot guarantee. Verify that
  the docs and `.pyi` describe what the installed extension actually exports.
  The implemented P0–P3 readers remain exposed, including process/core and I/O.
  The existing installed contract suite compares stub classes with `__all__`,
  checks each declared member exists and has native documentation, and exercises
  CPU, memory, swap/zswap, NUMA, process/core, and I/O readers. It now also
  checks the installed `py.typed` marker and extension placement. Snapshot
  immutability, guarded `MaxOr.value`, units, missing-file paths, and reader
  handle retention passed. Existing documentation correctly states that fresh
  reads are not atomic together; this review does not freeze future releases.
  [Binding contract](../sakai-python/python/sakai/_sakai.pyi).
- [x] Verify baseline version and core metadata consistency. Maturin derives
  `0.1.0` from Cargo workspace metadata; both wheel filenames and the sdist agree.
  All three archives have `Name: sakai`, `Version: 0.1.0`,
  `Requires-Python: >=3.11`, no `Requires-Dist`, `License-Expression: MIT`,
  and `License-File: LICENSE`.
- [ ] Set one release version consistently in Python metadata, Rust extension
  metadata, wheel/sdist filenames, and the release tag. Check that `Name`,
  `Version`, `Requires-Python`, and dependencies agree across **all** archives
  in the release. [Core metadata](https://packaging.python.org/en/latest/specifications/core-metadata/).
- [ ] Set `requires-python = ">=3.11"` and accurate Linux/CPython classifiers.
  Do not use classifiers as a substitute for `Requires-Python`; the latter
  controls install compatibility.
  The requirement is already set and verified in all baseline archives; Linux
  and Python 3 classifiers are present. A CPython implementation classifier
  is absent. Its addition is deferred with package-information work, not an
  install-compatibility failure; wheel tags already restrict the implementation.
  [Pyproject guidance](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/).
- [ ] Provide a Python-facing README with installation, a working cgroup v2
  example, Linux-only availability, missing-interface behavior, units, and
  fresh-read semantics. Add source, documentation, issue tracker, and changelog
  URLs to metadata. Ensure the package description renders correctly.
  [PyPI-friendly README](https://packaging.python.org/en/latest/guides/making-a-pypi-friendly-readme/),
  [PyPI project URLs](https://docs.pypi.org/project_metadata/).
- [ ] Decide which Linux architectures and libc environments the first release
  promises. Document the supported wheel matrix and what happens when no wheel
  matches (source build requirements or unsupported target). Do not label a
  host-specific `linux_*` wheel as broadly portable.
  The candidate matrix above passed native audits and installed tests. A source
  build needs Linux cgroup v2 for live use, regular CPython ≥3.11, Rust ≥1.88,
  a native compiler/linker, and the Maturin backend. The isolated source rebuild
  below used Rust 1.99.0 and Python 3.11; it does not establish an extension
  build on Rust 1.88 or support for every target without a matching wheel.
  [Maturin distribution](https://www.maturin.rs/distribution),
  [platform tags](https://packaging.python.org/en/latest/specifications/platform-compatibility-tags/).

## 2. Prove the installed behavior

- [ ] Run the repository's Rust gates (`devenv test`), including parser and
  Linux live tests, against the release candidate. Run Python lint, type,
  and contract checks added with the binding.
  Baseline preparation passed `devenv --profile python test`: Rust formatting,
  Clippy, 43 local workspace tests, seven doctests, Ruff lint/format, and Ty.
  Linux Rust 1.99 / Python 3.11 also passed all 15 binding unit tests and binding
  Clippy with warnings denied. The merged parent `3b0d67751cb0e4e8e1a960467666c525dcbbdf25`
  passed [CI run 38113844116](https://github.com/risk1996/sakai/actions/runs/38113844116),
  including Rust/MSRV gates, Python 3.11/3.15 wheel and sdist tests,
  Kubernetes Python tests, security checks, and Linux 5.15/6.18 live tests.
  CI has not run on the unpushed baseline commit; rerun on the release candidate.
- [x] Test the **installed baseline wheels**, outside the checkout, on CPython 3.11 and
  every newer minor version the release claims. Check import, public exports,
  `py.typed`/stubs, constructors and child traversal, every bound CPU/memory/core
  reader, exception paths, and conversion of `u64` values above signed 64-bit
  range where representable. Ensure tests cannot accidentally import the
  source tree. [Wheel install behavior](https://packaging.python.org/en/latest/flow/).
  Both architectures passed on every minor from 3.11 through stable 3.15.0.
  Each run discovered 27 tests: 23 passed and four Kubernetes-only tests were
  skipped intentionally. Containers ran as UID/GID 65532, without capabilities
  or networking, with only archives and copied tests mounted read-only; Python
  ran with `-I` in a clean venv under `/tmp`. No checkout was available.
  Python-visible `u64::MAX` conversions were verified separately by the 15
  Rust binding tests, not induced through host cgroup values.
- [x] Test baseline Linux cgroup v2 live reads without elevated privileges.
  Installed tests exercised root exemptions, optional/missing interfaces,
  read-only snapshots, and every bound reader on the Docker Linux host.
  Existing Linux 5.15/6.18 Rust live tests passed in the merged parent CI;
  those VM jobs do not run the Python extension. No new/intermediate-kernel
  matrix or macOS CI was added.
- [ ] Exercise process migration or cgroup disappearance during a live query.
  Deleted-membership fixtures and binding error-conversion tests cover the
  corresponding error contract, but do not constitute a live race test. This
  preparation did not move processes or create/delete host cgroups.
  [Repository semantics](../README.md), [binding contract](../sakai-python/README.md).
- [x] Test invalid inputs and boundaries: PID 0/negative/overflow, non-cgroup
  paths, child traversal/symlinks, non-UTF-8 path round trips, parse errors,
  and permission errors. Assert Python exception classes and attributes, not
  only messages. [Binding error contract](../sakai-python/src/linux.rs).
  Installed tests cover invalid PIDs/paths/traversal and non-UTF-8 missing-child
  lookups (`FileNotFoundError`, `errno == ENOENT`). Existing core tests cover
  symlink rejection with both handle backends. Binding unit tests assert exact
  exception categories, `SakaiError` inheritance, diagnostic `.path`,
  permission/missing-directory `.errno`, non-UTF-8 round trips, immutable values,
  and unsigned values through `u64::MAX`. Parse and permission failures here
  are deterministic conversion tests, not malformed or chmod'ed cgroup files.
- [x] If the extension declares free-threaded safety through current PyO3
  defaults, audit shared native state and verify behavior on a supported
  free-threaded interpreter, even if no free-threaded wheel will be uploaded.
  Alternatively configure the module explicitly to require the GIL and
  document that choice. An `abi3` wheel itself does not support free-threaded
  CPython. Both Linux and non-Linux module declarations now explicitly use
  `#[pymodule(gil_used = true)]`, matching the existing regular-CPython-only
  contract. The opted-out build makes no free-threaded safety claim.
  [PyO3 0.29.3 free-threading](https://pyo3.rs/v0.29.3/free-threading),
  [PyO3 ABI guidance](https://pyo3.rs/v0.29.3/building-and-distribution).

## 3. Validate the exact distributions

- [x] Build baseline candidates from the same clean source commit. Rust 1.99.0,
  Maturin 1.15.0, and Zig 0.17.0 produced one sdist and both release-mode
  candidate wheels. Wheel builds used `--locked`, `--zig`,
  `--compatibility manylinux2014`, and `--auditwheel check`. Maturin's separate
  `sdist` command has no `--locked` option; the source lockfile was unchanged,
  but its extracted-workspace problem is recorded below. Local archives and
  their digests are preserved; these are not immutable release CI outputs.
- [ ] Build one sdist and all intended release wheels from the same reviewed
  commit with locked dependencies and a recorded toolchain. Store the artifacts
  as immutable CI outputs for promotion to TestPyPI and PyPI; do not rebuild
  different bytes after testing. [Maturin distribution](https://www.maturin.rs/distribution),
  [Python package formats](https://packaging.python.org/en/latest/discussions/package-formats/).
- [x] Build baseline Linux wheels in a suitable `manylinux` environment (or supported
  equivalent) and keep Maturin's native dependency audit enabled. Check the
  actual wheel filename and tags: `cp311-abi3` plus the intended
  `manylinux_*_<arch>` platform. Inspect any bundled shared libraries and
  required glibc baseline. [Maturin distribution](https://www.maturin.rs/distribution),
  [platform tags](https://packaging.python.org/en/latest/specifications/platform-compatibility-tags/).
  Maturin's documented Zig equivalent targeted glibc 2.17 from Debian bookworm.
  Both wheel filenames and internal tags are `cp311-abi3`, with
  `manylinux_2_17_<arch>` and `manylinux2014_<arch>` tags. `auditwheel show`
  confirmed the platform policy and system-library dependencies; no bundled
  shared libraries were needed. `abi3audit --strict --verbose` scanned each
  candidate and sdist-rebuilt extension: zero ABI mismatches or violations.
- [x] Inspect the contents of every baseline wheel: importable `sakai` package and
  `_sakai` extension, `__init__.py`, `.pyi`, `py.typed`, license, and metadata;
  no development caches, local paths, credentials, unrelated binaries, or
  stale libraries. Verify the module initialization name matches `_sakai`.
  Each wheel has nine files: the four package files above, METADATA, WHEEL,
  RECORD, MIT license, and Maturin's CycloneDX SBOM. `check-wheel-contents`
  passed; RECORD hashes/sizes and exact license text were checked.
  Both native builds export `PyInit__sakai`. The SBOM contains generic
  `path+file:///src/sakai` and `/src/sakai-python` component identifiers;
  these reviewed build-root identifiers are an exception to the local-path
  exclusion, not host paths or credentials. No caches, extra native libraries,
  personal paths, or unrelated workspace tools were included.
  [Maturin layout](https://www.maturin.rs/project_layout),
  [PyO3 modules](https://pyo3.rs/main/module).
- [ ] Inspect the sdist for the nested workspace path dependency: it must
  include `sakai-python`, `sakai`, relevant workspace manifests/lockfile,
  Python source, README, and licenses. In an isolated Linux environment with
  no checkout, build a wheel **from that sdist**, install it, and rerun the
  smoke/contract suite. [Maturin sdist guidance](https://www.maturin.rs/distribution),
  [package formats](https://packaging.python.org/en/latest/discussions/package-formats/).
  The baseline's 86 entries include both crates, root/workspace manifests and
  lockfile, Python package/stubs/tests, binding README, and all licenses; no
  `xtask` sources are included. A direct extracted `--locked` build **fails**:
  Maturin removes `xtask` from workspace members but retains its lock entries.
  Offline `cargo metadata` pruned 216 packages to 57; retained versions, sources,
  checksums, and dependency targets stayed unchanged (three Windows references
  lost now-unnecessary version disambiguation). After that explicit normalization,
  both architectures rebuilt offline with `--locked`, audited successfully,
  and passed the installed 23-test suite on Python 3.11 (four Kubernetes skips).
  The original archive was not rewritten. Resolve the direct locked-build
  failure or explicitly approve this normalization policy before release;
  a normalized-copy check alone does not close this gate.
  A separate offline `pip install` from the untouched sdist also built and
  passed all 23 installed contract tests (four Kubernetes skips), with a
  preinstalled backend and `--no-build-isolation`. That normal source-install
  route permits Cargo to normalize the lockfile and produced a host-specific
  `linux_aarch64` wheel; it is not a portable release wheel or a locked-build pass.
- [ ] Run `twine check` on the final sdist and every wheel; inspect the rendered
  description and extracted metadata, then compare `Requires-Python` and
  dependency metadata across artifacts.
  Twine 7.0.0 checked all baseline archives and both sdist-rebuilt wheels:
  passed **with warnings** for missing long description and content type.
  Metadata matches across the three candidates. No description rendering or
  strict Twine pass is claimed; README/metadata work remains deferred.
  [PyPI-friendly README](https://packaging.python.org/en/latest/guides/making-a-pypi-friendly-readme/),
  [core metadata](https://packaging.python.org/en/latest/specifications/core-metadata/).
- [x] Install each baseline wheel by filename or from a local wheel directory in a clean
  environment with the source checkout unavailable. Confirm Python 3.11 accepts
  it, an unsupported interpreter/platform does not select it, and a source
  install behaves as documented. [Packaging flow](https://packaging.python.org/en/latest/flow/),
  [platform tags](https://packaging.python.org/en/latest/specifications/platform-compatibility-tags/).
  Both architectures installed and passed on Python 3.11–3.15 as above;
  source-rebuilt wheels passed on 3.11 after the recorded normalization.
  Python 3.10 rejected the wheel. Offline, wheel-only `pip download` rejected
  Windows, macOS, musl, ppc64le, and PyPy target selections. Packaging 26.3 tag
  checks also exclude these wheels from free-threaded CPython 3.13–3.15.
  These checks do not promise a successful source fallback on unsupported targets.

## 4. Exercise the release route before production

- [ ] Create a release workflow restricted to the intended version tags and
  reviewed source commits. Keep publishing out of pull-request jobs. Use a
  dedicated GitHub environment with maintainers as approvers and protect
  release tags. Give `id-token: write` only to the upload job; keep other jobs
  at read-only permissions. [PyPI Trusted Publisher setup](https://docs.pypi.org/trusted-publishers/using-a-publisher/),
  [security guidance](https://docs.pypi.org/trusted-publishers/security-model/).
- [ ] Configure PyPI and TestPyPI Trusted Publishers for the **exact** GitHub
  owner/repository, workflow filename, and environment. For a first PyPI
  release, use a pending publisher if the project does not yet exist; remember
  this does not reserve the name. Avoid long-lived upload tokens in the
  repository or CI. [PyPI Trusted Publishing](https://docs.pypi.org/trusted-publishers/),
  [pending publisher](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/).
- [ ] Run the release workflow against TestPyPI with the candidate artifacts,
  then install the uploaded wheel from **TestPyPI** into a fresh Python 3.11
  environment and run a short live smoke test. TestPyPI and PyPI are separate
  indices/accounts, so verify both publisher configurations. Use dependency
  resolution from PyPI only if needed, and inspect what index supplied each
  dependency. [Using TestPyPI](https://packaging.python.org/en/latest/guides/using-testpypi/),
  [Trusted Publisher TestPyPI example](https://docs.pypi.org/trusted-publishers/using-a-publisher/).
- [ ] Confirm the production upload job will consume exactly the artifacts
  that passed inspection and TestPyPI, and that no further code or metadata
  change requires rebuilding. Record filenames and SHA-256 digests for the
  release review. PyPI's default publish attestations can then bind the
  uploaded files to the Trusted Publisher identity.
  [PyPI attestations](https://docs.pypi.org/attestations/),
  [automatic publish attestations](https://docs.pypi.org/attestations/producing-attestations/).

## Final go/no-go

Proceed to production PyPI only when every applicable box above is checked,
the distribution name and ownership are confirmed, the exact archives are
preserved, and the release workflow is ready to upload them. A failed artifact
or TestPyPI check means fix the cause, build a new candidate, and repeat the
artifact checks. This checklist ends at the production upload; post-publication
installation, project-page review, and provenance verification belong in the
release runbook.

## Repeat the baseline verification

Use existing tools rather than a new packaging script. Run repository gates with
`devenv --profile python test`; inspect the matching commit's Linux CI jobs before
reusing their results. Keep `sakai-python/tests` as the installed contract suite.
Never upload these verification candidates or turn a successful baseline into
a final-release checkbox without rerunning the applicable gates.

For the Linux build, use a clean source tree mounted read-only at `/src`, writable
artifact storage at `/verification`, and a separate Cargo cache. The recorded
builder was `rust:1.99.0-bookworm` on aarch64 with Python 3.11 development headers,
both Rust targets, and a Python tools venv. Install the following tool versions
in that disposable builder (not in the package's runtime dependencies):

```text
python3 -m venv /verification/tools
/verification/tools/bin/pip install 'maturin[zig,patchelf]==1.15.0' 'ziglang==0.17.0' 'twine==7.0.0' 'check-wheel-contents==0.6.3' 'auditwheel==6.8.2' 'abi3audit==0.0.26' 'uv==0.13.0' 'packaging==26.3'
rustup target add aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu
```

Commands inside the builder, with `/verification/tools/bin` on `PATH`,
`CARGO_HOME` pointing to the separate cache, and `CARGO_TARGET_DIR` outside `/src`:

```text
cd /src
maturin build --release --locked --manifest-path sakai-python/Cargo.toml --target aarch64-unknown-linux-gnu --zig --compatibility manylinux2014 --auditwheel check --interpreter /usr/bin/python3 --out /verification/dist
maturin build --release --locked --manifest-path sakai-python/Cargo.toml --target x86_64-unknown-linux-gnu --zig --compatibility manylinux2014 --auditwheel check --interpreter /usr/bin/python3 --out /verification/dist
maturin sdist --manifest-path sakai-python/Cargo.toml --out /verification/dist
twine check /verification/dist/*
check-wheel-contents /verification/dist/*.whl
abi3audit --strict --verbose /verification/dist/*.whl
auditwheel show /verification/dist/sakai-0.1.0-cp311-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl
auditwheel show /verification/dist/sakai-0.1.0-cp311-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl
sha256sum /verification/dist/*
```

Derive filenames from the actual build output if the version or matrix changes.
Inspect wheel listings and METADATA/WHEEL with `unzip -l` / `unzip -p`, and sdist
listings with `tar -tzf`. Compare Name, Version, Requires-Python, Requires-Dist,
license metadata/text, package files, and native module initialization; review
the SBOM as well. Strict Twine (`twine check --strict`) must pass once the README
and metadata are supplied; the recorded warning-only run is not that check.

Copy only the contract tests into artifact storage. For each architecture and
CPython minor, use a **new** container with explicit `--platform linux/arm64` or
`linux/amd64`, `--user 65532:65532`, `--cap-drop ALL`, `--network none`, working
directory `/tmp`, and only that artifact storage mounted read-only. Do not mount
the checkout. Inside each test container:

```text
python -V
python -m venv /tmp/sakai-wheel
/tmp/sakai-wheel/bin/pip install --no-cache-dir --no-index --no-deps /verification/dist/<matching-wheel>.whl
/tmp/sakai-wheel/bin/python -I -m unittest discover -s /verification/tests
```

The recorded x86_64 runtimes were Python 3.11.17, 3.12.14, 3.13.15, 3.14.7,
and 3.15.0; aarch64 used 3.11.17, 3.12.15, 3.13.16, 3.14.8, and 3.15.0.
Docker's stable 3.15 bookworm tag was unavailable at verification time; stable
3.15.0 came from `uv python install cpython-3.15.0-linux-<arch>-gnu` instead.
Download interpreters in the networked preparation step, then mount them
read-only and use their full executable paths in offline test containers.

For compatibility rejection, repeat the filename install on Python 3.10 and
expect a nonzero exit. On a supported interpreter, use the following command
with each unsupported platform (`win_amd64`, `macosx_11_0_x86_64`,
`musllinux_1_2_x86_64`, `manylinux_2_17_ppc64le`) and expect no matching wheel:

```text
pip download --no-cache-dir --no-index --no-deps --only-binary=:all: --find-links /verification/dist --platform <platform> --python-version 3.11 --implementation cp --abi abi3 --dest /tmp/downloads sakai==0.1.0
```

Repeat with the x86_64 manylinux platform, `--implementation pp`, and
`--abi pypy311_pp73` for PyPy. For free-threaded runtimes, compare the wheel tags
against `packaging.tags.cpython_tags` for Python 3.13–3.15 with only `cp313t`,
`cp314t`, or `cp315t` respectively; `cp311-abi3` must not be selected.

Extract the original sdist into fresh artifact storage and start a separate
Linux builder with **no checkout mount**. First attempt the corresponding
`maturin build --release --locked --offline ...` from the extracted tree.
The recorded failure must remain visible; do not silently remove `--locked`.
For diagnostic normalization, preserve the original Cargo.lock, run
`cargo metadata --offline --format-version 1`, and compare parsed lockfiles:
retained package name/version/source/checksum and resolved dependency targets
must be identical; only unused packages/disambiguation may disappear. If a
retained dependency changes, stop rather than claiming the locked graph passed.
Then run both Maturin wheel builds above with `--locked --offline` from the
extracted tree, a fresh target directory, and output to `/verification/from-sdist`.
Repeat audits and clean installed tests on those wheels. A networkless container
requires the backend/compiler/dependency cache to be prepared beforehand; fetch
any missing dependencies using the extracted manifest and its approved lockfile
in a networked preparation container, never during the offline check.

Separately test normal source installation from the untouched tarball in a fresh
venv/builder, with no checkout mount. Prepare the declared Maturin backend in
that venv before disabling networking, set `CARGO_NET_OFFLINE=true`, and use
`pip install --no-cache-dir --no-index --no-deps --no-build-isolation <sdist>`.
Run the same installed contract suite. This verifies the PEP 517 route, but
does not replace the stricter `--locked` source-rebuild gate above.

The baseline archives are preserved locally at
`/private/tmp/sakai-pypi-artifacts.efTWIg/dist`; normalized diagnostic source and
rebuilds are separate under that directory. Temporary local storage is not an
immutable CI release artifact. Record digests before promotion; these identify
the original three candidates, not rebuilt or future release files:

```text
a98591b3cbf50ed836778c8e53eff764d24998eec59c5a66d70365d2dba37fd0  sakai-0.1.0-cp311-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl
18db915b5e856d2d94f89328ee915791d66f92b7ad1ed5f7baaf29ffbd815d81  sakai-0.1.0-cp311-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl
db4046d00671445bbffa8b1f84d69e175b21703b5b068d5bd17e7bc730a2b501  sakai-0.1.0.tar.gz
```

The wheel sizes were 506,329 bytes (aarch64) and 545,862 bytes (x86_64);
the sdist was 83,797 bytes. Preserve source commit and tool versions with any
future artifacts, and repeat these checks after code, metadata, or tool changes.
