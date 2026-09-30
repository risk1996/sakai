# PyPI pre-publication checklist

Research checked on 2026-09-30. Use this **after implementing** the bindings
described in [pyo3-bindings.md](pyo3-bindings.md) and **before uploading a
release to production PyPI**. Every item is deliberately unchecked: this
repository does not yet have a Python extension or release artifacts. A
successful local import is not the release gate; test the actual archives that
will be uploaded.

First release target: Linux CPython, Python 3.10 minimum,
`abi3-py310`, package import `sakai`, built with Maturin. Do not silently add
macOS/Windows wheels or claim free-threaded CPython support while the Linux-only
handle API and ABI plan remain as documented. Sources for these package-specific
choices are the binding plan and current Rust code; the external best-practice
sources are linked below.

## 1. Settle the public release contract

- [ ] Confirm the distribution name with PyPI immediately before release, its
  normalized spelling, and that the project owner controls it. Align the name
  in `pyproject.toml`, documentation, release workflow, and PyPI Trusted
  Publisher configuration. A pending publisher does **not** reserve the name.
  [PyPI project creation](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/),
  [name rules](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/).
- [ ] Choose and add a license for the Rust and Python code; include the actual
  license file(s) in both wheel and sdist. Record a valid SPDX expression and
  `license-files` in Python metadata; make `sakai-core` metadata consistent.
  This is unfinished in [project-direction.md](project-direction.md).
  [Python metadata specification](https://packaging.python.org/en/latest/specifications/pyproject-toml/).
- [ ] Freeze and review the initial Python surface: class and method names,
  unit suffixes, `None` for a successful `max` reading, optional older-kernel
  fields, exception types, and the no-atomic-snapshot guarantee. Verify that
  the docs and `.pyi` describe what the installed extension actually exports.
  [Binding contract](pyo3-bindings.md).
- [ ] Set one release version consistently in Python metadata, Rust extension
  metadata, wheel/sdist filenames, and the release tag. Check that `Name`,
  `Version`, `Requires-Python`, and dependencies agree across **all** archives
  in the release. [Core metadata](https://packaging.python.org/en/latest/specifications/core-metadata/).
- [ ] Set `requires-python = ">=3.10"` and accurate Linux/CPython classifiers.
  Do not use classifiers as a substitute for `Requires-Python`; the latter
  controls install compatibility.
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
  [Maturin distribution](https://www.maturin.rs/distribution),
  [platform tags](https://packaging.python.org/en/latest/specifications/platform-compatibility-tags/).

## 2. Prove the installed behavior

- [ ] Run the repository's Rust gates (`devenv test`), including parser and
  Linux live tests, against the release candidate. Run Python lint, type,
  and contract checks added with the binding.
- [ ] Test the **installed wheel**, outside the checkout, on CPython 3.10 and
  every newer minor version the release claims. Check import, public exports,
  `py.typed`/stubs, constructors and child traversal, every bound CPU/memory/core
  reader, exception paths, and conversion of `u64` values above signed 64-bit
  range where representable. Ensure tests cannot accidentally import the
  source tree. [Wheel install behavior](https://packaging.python.org/en/latest/flow/).
- [ ] Test Linux cgroup v2 live reads without elevated privileges, plus the
  existing older-kernel VM matrix where file availability varies. Cover root
  exemptions, controller-disabled/missing files, optional counters, read-only
  behavior, and a process moving or cgroup disappearing during a query.
  [Repository semantics](../README.md), [binding contract](pyo3-bindings.md).
- [ ] Test invalid inputs and boundaries: PID 0/negative/overflow, non-cgroup
  paths, child traversal/symlinks, non-UTF-8 path round trips, parse errors,
  and permission errors. Assert Python exception classes and attributes, not
  only messages. [Binding error contract](pyo3-bindings.md).
- [ ] If the extension declares free-threaded safety through current PyO3
  defaults, audit shared native state and verify behavior on a supported
  free-threaded interpreter, even if no free-threaded wheel will be uploaded.
  Alternatively configure the module explicitly to require the GIL and
  document that choice. An `abi3` wheel itself does not support free-threaded
  CPython. [PyO3 free-threading](https://pyo3.rs/main/free-threading.html),
  [PyO3 ABI guidance](https://pyo3.rs/main/building-and-distribution).

## 3. Validate the exact distributions

- [ ] Build one sdist and all intended release wheels from the same reviewed
  commit with locked dependencies and a recorded toolchain. Store the artifacts
  as immutable CI outputs for promotion to TestPyPI and PyPI; do not rebuild
  different bytes after testing. [Maturin distribution](https://www.maturin.rs/distribution),
  [Python package formats](https://packaging.python.org/en/latest/discussions/package-formats/).
- [ ] Build Linux wheels in a suitable `manylinux` environment (or supported
  equivalent) and keep Maturin's native dependency audit enabled. Check the
  actual wheel filename and tags: `cp310-abi3` plus the intended
  `manylinux_*_<arch>` platform. Inspect any bundled shared libraries and
  required glibc baseline. [Maturin distribution](https://www.maturin.rs/distribution),
  [platform tags](https://packaging.python.org/en/latest/specifications/platform-compatibility-tags/).
- [ ] Inspect the contents of every wheel: importable `sakai` package and
  `_sakai` extension, `__init__.py`, `.pyi`, `py.typed`, license, and metadata;
  no development caches, local paths, credentials, unrelated binaries, or
  stale libraries. Verify the module initialization name matches `_sakai`.
  [Maturin layout](https://www.maturin.rs/project_layout),
  [PyO3 modules](https://pyo3.rs/main/module).
- [ ] Inspect the sdist for the nested workspace path dependency: it must
  include `sakai-python`, `sakai-core`, relevant workspace manifests/lockfile,
  Python source, README, and licenses. In an isolated Linux environment with
  no checkout, build a wheel **from that sdist**, install it, and rerun the
  smoke/contract suite. [Maturin sdist guidance](https://www.maturin.rs/distribution),
  [package formats](https://packaging.python.org/en/latest/discussions/package-formats/).
- [ ] Run `twine check` on the final sdist and every wheel; inspect the rendered
  description and extracted metadata, then compare `Requires-Python` and
  dependency metadata across artifacts.
  [PyPI-friendly README](https://packaging.python.org/en/latest/guides/making-a-pypi-friendly-readme/),
  [core metadata](https://packaging.python.org/en/latest/specifications/core-metadata/).
- [ ] Install each wheel by filename or from a local wheel directory in a clean
  environment with the source checkout unavailable. Confirm Python 3.10 accepts
  it, an unsupported interpreter/platform does not select it, and a source
  install behaves as documented. [Packaging flow](https://packaging.python.org/en/latest/flow/),
  [platform tags](https://packaging.python.org/en/latest/specifications/platform-compatibility-tags/).

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
  then install the uploaded wheel from **TestPyPI** into a fresh Python 3.10
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
