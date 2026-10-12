# Project direction

Sakai is working toward a publishable Rust crate for read-only cgroup v2
metrics. The remaining interface work and its priority order live in
[`TODO.md`](../TODO.md). Linux handles expose borrowed views for every supported
controller and core interfaces, with matching Python readers;
parsers and snapshot values remain available on other platforms.

## Rust crate publication

Once the read surface and public API are ready, prepare `sakai` for its
first crates.io release. The implemented preparation and manual bootstrap
sequence are documented in [releases.md](releases.md); the first upload,
trusted-publisher registration, and tag push still require the owner:

- Verify package metadata and the README installation and usage
  example that matches the released API.
- Check the packaged file list and run the existing project checks against the
  release candidate.
- Verify the existing tag-gated release workflow. Keep publishing out of pull-request jobs,
  and verify the release process before the first public version.
