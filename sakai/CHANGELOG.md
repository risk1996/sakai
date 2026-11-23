# Changelog

## [0.1.0-alpha.2]

### Added

- First PyPI release. Linux wheels for regular CPython 3.11 and newer support
  x86_64 and AArch64 with glibc 2.17 or newer.

## [0.1.0-alpha.1]

### Added

- Read-only cgroup v2 discovery and pinned Linux handles, including CPU, memory,
  swap/zswap, process-count, core, I/O, cpuset placement, and Linux notification
  support.
- Portable parsers and owned snapshots for saved cgroup interface contents.
- Typed quantities, unlimited values, pressure metrics, and structured errors
  that preserve missing interfaces and operating-system failures.
- Python bindings for the implemented Linux readers. PyPI publishing is being
  prepared separately; this release workflow publishes only the Rust crate.

### Compatibility

- Rust 1.88 minimum; Linux 5.15 is the oldest tested kernel.
- Missing optional counters remain distinct from zero, and separate reads are
  not an atomic snapshot.
- Compatible additions and fixes can remain in 0.1.x. Breaking public API or
  behavioral changes require 0.2.0.
