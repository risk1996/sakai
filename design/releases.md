# Sakai releases

Pushing a matching version tag to a commit on `main` starts the
[Release workflow](../.github/workflows/release.yml). GitHub Actions runs
the release checks, publishes to crates.io and PyPI after approval, and
creates the GitHub release once both registries are verified.

## Prepare and tag

1. Update the shared version in [Cargo.toml](../Cargo.toml), refresh
   `Cargo.lock`, and add the version section to the
   [changelog](../sakai/CHANGELOG.md). Maturin derives the Python version
   from Cargo, for example `0.1.0-alpha.2` becomes `0.1.0a2`.
   After the first stable tag, [Release preparation](../.github/workflows/release-pr.yml)
   can open a PR for these changes.
2. Merge the release changes and wait for CI on the merged commit to pass.
3. Create an annotated tag named `v<workspace-version>` on that exact commit
   and push the tag. For example, with `version` and `commit` set to the
   release version and the tested commit SHA:

   ```sh
   rtk git tag -a "v${version}" "$commit" -m "Release v${version}"
   rtk git push origin "refs/tags/v${version}"
   ```

## Review the release archives

In the **Release** run, review the crate, wheels, source archive, and
checksums. The workflow builds and tests the distributions for this tag.
Approve the `crates-io` and `pypi` deployments to publish through Trusted
Publishing.

## Rehearse or retry

- To rehearse on an existing version tag, dispatch **Release** with
  `publish: false`. It builds and checks the archives without publishing.
- If a publish fails, rerun the failed jobs for the same tag to reuse the
  checked artifacts. If the candidate needs changes, prepare a new version
  and tag.
