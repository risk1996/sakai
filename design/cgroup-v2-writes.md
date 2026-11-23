# Implementing cgroup v2 writes

## Starting point

Build on the shared codecs from [PR #72](https://github.com/risk1996/sakai/pull/72)
and the [codec ADR](../adr/shared-cgroup-text-codecs.md). Use the local codec
branch as the base if it has changes that the write implementation needs and
those changes are still unpublished.

The controller views borrow an open [`Cgroup`](../sakai/src/cgroup/v2/handle.rs).
Reads open an interface relative to that directory descriptor, collect its
contents, and parse them. The [text codecs](../sakai/src/parse/codec.rs) turn
typed values into canonical strings and can reject inexact encodings. They
also encode read-only snapshots, so having an encoder says nothing about
which operations the kernel accepts.

## First implementation

Start with two Linux-only controller methods. These signatures are proposals.

```rust,ignore
impl Cpu<'_> {
  pub fn set_max(&self, value: &CpuMax) -> Result<(), Error>;
}

impl Memory<'_> {
  pub fn set_max(&self, value: MaxOr<Bytes>) -> Result<(), Error>;
}
```

* `Cpu::set_max` replaces both quota and period using
  [`CpuMax::encode_cgroup()`](../sakai/src/cgroup/v2/cpu/max.rs). If callers
  need to build a value programmatically, add a constructor taking the
  existing typed quantities and rejecting sub-microsecond remainders.
  Keep the snapshot parser's accepted range intact. The kernel validates
  settings against scheduler constraints and the current hierarchy.
* `Memory::set_max` can turn its typed limit into a
  [`MemoryMax`](../sakai/src/cgroup/v2/memory.rs) with the existing crate-private
  [`Scalar::new`](../sakai/src/scalar.rs) and use the scalar encoder. Test
  finite page-aligned limits and `max`. Other values may be normalized to page
  granularity on readback. Lowering a live limit can initiate reclaim or OOM
  handling, so use an empty scratch cgroup for the first live tests.
  [Linux CPU and memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#interface-files)

Expose explicit methods on the controller views. Each method owns its
choice of interface and command type, and uses the existing `FILE_NAME`
constant. If access metadata later becomes useful for multiple callers,
associate snapshot and command types with an interface descriptor. A
shared text codec does not require a generic public write API.

## Transport and errors

1. Finish encoding and validating the entire command before opening the
   file. Pass encoding failures through a typed error with the interface
   path and the original `EncodeError` as its source.
2. Add a crate-private write companion to
   [`read_file`](../sakai/src/cgroup/v2/file.rs). Share the single-filename
   validation with the reader. Use `openat` on the held cgroup descriptor
   with `WRONLY | CLOEXEC | NOFOLLOW`. Use the stored path for diagnostics.
   Open the existing interface without create, truncate, or append flags.
3. Send one complete command with one `write(2)`. Treat a zero or short
   result for a nonempty command as an error. A second write of the
   remainder would be interpreted as another command, so avoid
   `write_all` and buffered writers. If retrying `EINTR`, retry the
   entire command only when the syscall reports interruption before
   writing bytes. [Linux `write(2)`](https://man7.org/linux/man-pages/man2/write.2.html)
4. Map filesystem and kernel failures at the handle layer. Follow the
   [existing error conventions](../sakai/src/error.rs) for missing files
   and unsupported operations. Preserve the path, the underlying errno,
   and the write operation in other errors. Check permission error
   mappings against caller expectations for delegation and read-only
   mounts. Report success once the full command is accepted. Readback is
   a separate operation and can observe normalization or another writer.

Use the codec's canonical bytes for the first two setters. Later commands
may need interface-specific framing. For example, an empty placement list
for an interface that supports clearing needs a nonempty write such as a
newline, after confirming the kernel's accepted syntax. A zero-length
syscall cannot serve as an explicit clear.

## Extend by command semantics

| Interface | Write design |
| --- | --- |
| `cpu.max` quota-only update | If exposed, give it a dedicated command using the shared quota token codec. The kernel supports updating quota with one value. Avoid a userspace read-modify-write that could overwrite a concurrent period change. |
| `cpu.weight` and `cpu.idle` | Accept a validated `Weight` for setting shares. Represent the observed idle state as a separate operation against `cpu.idle`. |
| `io.max` | Use a typed patch for one device, with at least one requested change. Share field keys and units with the snapshot codec. An omitted field leaves its limit alone, while `max` clears it. A whole `IoMax` snapshot encodes multiple device rows and is unsuitable as a single command. |
| `io.weight` | Model the default weight, an individual device override, and resetting an override as distinct commands. |
| `cgroup.procs` and `cgroup.threads` | Use a migration method with one target ID per syscall. Model a request to move the caller explicitly if it is supported, since a stored zero can mean an ID was invisible to the reader. |
| `cgroup.subtree_control` | Use enable and disable actions with `+` and `-` prefixes. Keep their batch semantics explicit in the method. |
| PSI triggers and peak resets | Use dedicated APIs where behavior depends on the open file descriptor. PSI needs the same descriptor for registration and polling. Peak resets with descriptor-local semantics must read through the descriptor that performed the reset. |

See the [Linux cgroup v2 interface descriptions](https://docs.kernel.org/admin-guide/cgroup-v2.html#interface-files),
[process and thread migration](https://docs.kernel.org/admin-guide/cgroup-v2.html#organizing-processes-and-threads),
and [PSI monitor lifetime](https://docs.kernel.org/accounting/psi.html#monitoring-for-pressure-thresholds).

## Verification

* Unit-test the exact command bytes and the error when a value cannot be
  encoded. Test that the transport rejects unsafe names and final symlinks,
  opens existing files only, and never submits the remainder of a short write.
  Use a temporary directory or a controlled write stub for these cases.
* Add real-kernel coverage to [`linux_live.rs`](../sakai/tests/linux_live.rs)
  using its scratch cgroup and cleanup flow. Exercise set and readback for
  both first setters, plus a missing or rejected operation with a preserved
  path and errno. Skip controller-specific cases only when the fixture cannot
  supply that controller. The [vmtest ADR](../adr/e2e_live_linux_test.md)
  describes the existing multi-kernel test route.
* Run the workspace checks under `devenv`. Add Python entry points after the
  Rust signatures and error behavior are settled.
