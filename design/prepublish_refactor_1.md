# Prepublish refactor 1: completed

- Added Linux-only `CgroupPath::candidates` to select unified membership, reject deleted memberships, and order visible cgroup2 mounts by root depth. The candidates are `CgroupPath` values, so there is no parallel candidate struct or private error enum. `Cgroup::from_pid` opens candidates in order and verifies filesystem magic; `CgroupPath::current` exposes the first candidate.
- `CgroupPathError::DeletedCgroup { path }` is the only new public error variant. The handle maps missing membership or mount to `Error::NotCgroupV2`, deleted membership to `Error::DeletedCgroup`, and procfs failures to `Error::Io`.
- Added crate-private `common::parser::KeyedFields` for borrowed keyed values, complete-file error context, required and optional typed lookups, and presence checks. CPU stat and PSI keep their own tokenization and unknown-field rules; later values still replace earlier values for a repeated key.
- Added fixture and table coverage for unified, bind-mounted, namespace-root, deleted, and multiple matching mounts, including fallback after an open failure. Expanded CPU stat and PSI cases for malformed, missing, and overflowing values.
- Verified with `devenv test` on macOS and Linux library tests in the local Rust container. Stage 2 can move `KeyedFields` and its error types without a new public import.
