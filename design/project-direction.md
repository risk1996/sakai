# Project direction

Sakai is working toward a publishable Rust crate for read-only cgroup v2
metrics. The remaining interface work and its priority order live in
[`TODO.md`](../TODO.md). Linux handles expose borrowed CPU and core views;
parsers and snapshot values remain available on other platforms.

## Rust crate publication

Once the read surface and public API are ready, prepare `sakai-core` for its
first crates.io release:

- Add the license, package metadata, and a README installation and usage
  example that matches the released API.
- Check the packaged file list and run the existing project checks against the
  release candidate.
- Add a tag-gated release workflow. Keep publishing out of pull-request jobs,
  and verify the release process before the first public version.
