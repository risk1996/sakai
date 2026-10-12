//! Read-only event polling and configuration change notifications on Linux.
//!
//! Subscriptions retain kernel notification state independently of snapshot
//! readers. They do not write interfaces or enable controllers. Notifications
//! are hints to read a fresh snapshot, not a count of changes or an atomic
//! association with the values read later. Multiple changes can coalesce.
//!
//! Evented interfaces use `POLLPRI`/`POLLERR`; configuration watches use inotify
//! `IN_MODIFY` for writes. Automatic changes in effective limits or placement
//! need not produce configuration notifications. PSI files are excluded because
//! subscribing to pressure thresholds requires writing a trigger.
//!
//! See [cgroups(7)](https://man7.org/linux/man-pages/man7/cgroups.7.html),
//! [poll(2)](https://man7.org/linux/man-pages/man2/poll.2.html),
//! [inotify(7)](https://man7.org/linux/man-pages/man7/inotify.7.html), and
//! [Linux PSI triggers](https://docs.kernel.org/accounting/psi.html#monitoring-for-pressure-thresholds).

use std::{
  fs::File,
  io::{self, Seek, SeekFrom},
  mem::MaybeUninit,
  os::fd::{AsFd, AsRawFd, BorrowedFd},
  path::PathBuf,
  time::Duration,
};

use rustix::{
  event::{PollFd, PollFlags, Timespec, poll},
  fd::OwnedFd,
  fs::{Mode, OFlags, inotify, openat},
};

use crate::Error;

/// An interface with kernel cgroup event notifications.
///
/// Parses and displays exact filenames. The file must be available for this
/// cgroup; controllers, hierarchy roots, and kernel versions affect availability.
/// Notifications include changes in descendants where the interface is
/// hierarchical. See [Linux cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html).
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumString, strum::Display, strum::IntoStaticStr, strum::EnumIter,
)]
#[non_exhaustive]
pub enum EventInterface {
  /// Populated and frozen lifecycle changes for this cgroup and descendants.
  /// See [`super::core::CgroupEvents`] for field meanings and availability.
  #[strum(serialize = "cgroup.events")]
  CgroupEvents,
  /// Hierarchical memory protection, limit, and OOM events.
  /// See [`super::memory::MemoryEvents`] for field meanings and hierarchy options.
  #[strum(serialize = "memory.events")]
  MemoryEvents,
  /// Memory events in this cgroup only.
  /// See [`super::memory::MemoryEventsLocal`] for field meanings and availability.
  #[strum(serialize = "memory.events.local")]
  MemoryEventsLocal,
  /// Swap high, max, and allocation-failure events.
  /// See [`super::memory::SwapEvents`] for optional fields and availability.
  #[strum(serialize = "memory.swap.events")]
  MemorySwapEvents,
  /// Task-limit events; hierarchy behavior follows the cgroup mount options.
  /// See [`super::pids::PidsEvents`] for kernel-version and hierarchy details.
  #[strum(serialize = "pids.events")]
  PidsEvents,
  /// CPU partition state and validity changes, including hotplug effects.
  /// See [`super::cpuset::CpusetPartition`] for states and kernel-version details.
  #[strum(serialize = "cpuset.cpus.partition")]
  CpusetCpusPartition,
}

impl EventInterface {
  /// The exact cgroup v2 interface filename, suitable for diagnostic output.
  pub fn filename(&self) -> &'static str { self.into() }
}

/// A supported configuration interface whose writes can be watched with inotify.
///
/// Parses and displays exact filenames. A write notification does not guarantee
/// a different value; ancestor configuration and hotplug changes are not a
/// general source of notifications on these files. Availability follows the
/// corresponding reader, including optional and root-exempt interfaces.
/// See [Linux cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html).
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumString, strum::Display, strum::IntoStaticStr, strum::EnumIter,
)]
#[non_exhaustive]
pub enum ConfigurationInterface {
  /// Cgroup domain or threaded topology.
  /// See [`super::core::CgroupType`] for topology and availability.
  #[strum(serialize = "cgroup.type")]
  CgroupType,
  /// Controllers enabled for child cgroups.
  /// See [`super::core::Core::subtree_control`] for the snapshot reader.
  #[strum(serialize = "cgroup.subtree_control")]
  CgroupSubtreeControl,
  /// CPU quota and period in microseconds, including an unlimited quota.
  /// See [`super::cpu::CpuMax`] for units and availability.
  #[strum(serialize = "cpu.max")]
  CpuMax,
  /// CPU idle state or relative scheduling weight.
  /// See [`super::cpu::CpuWeight`] for valid weights and scheduling semantics.
  #[strum(serialize = "cpu.weight")]
  CpuWeight,
  /// Maximum utilization clamp, in percent, or `max`.
  /// See [`super::cpu::CpuUclampMax`] for units and availability.
  #[strum(serialize = "cpu.uclamp.max")]
  CpuUclampMax,
  /// Minimum utilization clamp, in percent.
  /// See [`super::cpu::CpuUclampMin`] for units and availability.
  #[strum(serialize = "cpu.uclamp.min")]
  CpuUclampMin,
  /// CPU quota burst duration in microseconds.
  /// See [`super::cpu::CpuMaxBurst`] for units and semantics.
  ///
  /// Since Linux 5.16; see the [Linux 5.16 source](https://github.com/torvalds/linux/blob/v5.16/Documentation/admin-guide/cgroup-v2.rst).
  #[strum(serialize = "cpu.max.burst")]
  CpuMaxBurst,
  /// CPU scheduling nice value.
  /// See [`super::cpu::Nice`] for the valid range and semantics.
  #[strum(serialize = "cpu.weight.nice")]
  CpuWeightNice,
  /// CPU idle scheduling policy.
  /// See [`super::cpu::CpuIdle`] for scheduling semantics and availability.
  #[strum(serialize = "cpu.idle")]
  CpuIdle,
  /// Hard memory limit in bytes, or `max`.
  /// See [`super::memory::MemoryMax`] for hierarchy and availability.
  #[strum(serialize = "memory.max")]
  MemoryMax,
  /// Memory throttling limit in bytes, or `max`.
  /// See [`super::memory::MemoryHigh`] for hierarchy and availability.
  #[strum(serialize = "memory.high")]
  MemoryHigh,
  /// Best-effort memory protection in bytes, or `max`.
  /// See [`super::memory::MemoryLow`] for protection semantics and availability.
  #[strum(serialize = "memory.low")]
  MemoryLow,
  /// Hard memory protection in bytes, or `max`.
  /// See [`super::memory::MemoryMin`] for protection semantics and availability.
  #[strum(serialize = "memory.min")]
  MemoryMin,
  /// Group OOM-kill policy.
  /// See [`super::memory::MemoryOomGroup`] for policy semantics and availability.
  #[strum(serialize = "memory.oom.group")]
  MemoryOomGroup,
  /// Hard swap limit in bytes, or `max`.
  /// See [`super::memory::SwapMax`] for hierarchy and availability.
  #[strum(serialize = "memory.swap.max")]
  MemorySwapMax,
  /// Swap throttling limit in bytes, or `max`.
  /// See [`super::memory::SwapHigh`] for throttling semantics and availability.
  #[strum(serialize = "memory.swap.high")]
  MemorySwapHigh,
  /// Zswap limit in bytes, or `max`.
  /// See [`super::memory::ZswapMax`] for hierarchy and availability.
  ///
  /// Since Linux 5.19; see the [Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/Documentation/admin-guide/cgroup-v2.rst).
  #[strum(serialize = "memory.zswap.max")]
  MemoryZswapMax,
  /// Zswap writeback policy.
  /// See [`super::memory::ZswapWriteback`] for hierarchy and availability.
  ///
  /// Since Linux 6.8; see the [Linux 6.8 source](https://github.com/torvalds/linux/blob/v6.8/Documentation/admin-guide/cgroup-v2.rst).
  #[strum(serialize = "memory.zswap.writeback")]
  MemoryZswapWriteback,
  /// Task-count limit, or `max`.
  /// See [`super::pids::PidsMax`] for limit semantics and availability.
  #[strum(serialize = "pids.max")]
  PidsMax,
  /// Default and per-device relative I/O weights.
  /// See [`super::io::IoWeight`] for device keys and availability.
  #[strum(serialize = "io.weight")]
  IoWeight,
  /// Per-device byte-per-second and operation-per-second limits.
  /// See [`super::io::IoMax`] for units, device keys, and availability.
  #[strum(serialize = "io.max")]
  IoMax,
  /// Per-device I/O latency targets in microseconds.
  /// See [`super::io::IoLatency`] for units and availability.
  #[strum(serialize = "io.latency")]
  IoLatency,
  /// Configured CPU ID list.
  /// See [`super::cpuset::CpuList`] for range syntax and effective lists.
  #[strum(serialize = "cpuset.cpus")]
  CpusetCpus,
  /// Configured NUMA memory-node ID list.
  /// See [`super::cpuset::MemoryNodeList`] for range syntax and effective lists.
  #[strum(serialize = "cpuset.mems")]
  CpusetMems,
  /// CPU partition state; also emits notifications for validity changes.
  /// See [`super::cpuset::CpusetPartition`] for states and kernel-version details.
  #[strum(serialize = "cpuset.cpus.partition")]
  CpusetCpusPartition,
}

impl ConfigurationInterface {
  /// The exact cgroup v2 interface filename, suitable for diagnostic output.
  pub fn filename(&self) -> &'static str { self.into() }
}

/// An independent event subscription holding a read-only interface descriptor.
///
/// Create it with [`super::Cgroup::poll_events`]. An initial read establishes a
/// baseline; changes after that read can make the first wait ready immediately.
/// Each successful wait rewinds and reads the same descriptor to acknowledge
/// its notification state. The contents are discarded without parsing them.
/// Fresh typed readers retain their existing read semantics.
///
/// Implements [`AsFd`] for external poll/epoll loops: request priority readiness
/// (`POLLPRI`/`EPOLLPRI`), then call [`Self::wait`] with a zero timeout to
/// acknowledge readiness. Ordinary readable readiness would spin on these files.
/// Dropping the subscription closes its descriptor. See
/// [cgroups(7)](https://man7.org/linux/man-pages/man7/cgroups.7.html).
#[derive(Debug)]
pub struct EventPoller {
  file: File,
  path: PathBuf,
  interface: EventInterface,
}

impl EventPoller {
  pub(crate) fn open(directory: impl AsFd, path: PathBuf, interface: EventInterface) -> Result<Self, Error> {
    let descriptor =
      openat(directory, interface.filename(), OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW, Mode::empty())
        .map_err(|error| Error::read(path.clone(), error.into()))?;
    let mut poller = Self { file: descriptor.into(), path, interface };
    poller.acknowledge()?;
    Ok(poller)
  }

  /// The evented interface selected when this subscription was created.
  pub fn interface(&self) -> EventInterface { self.interface }

  /// Waits for a change, acknowledging its notification state before returning.
  ///
  /// `None` waits indefinitely; a duration is measured in seconds with
  /// nanosecond precision, and zero checks readiness without blocking. `false`
  /// means the timeout expired, while `true` means at least one notification
  /// occurred. Notifications can coalesce. Read a fresh typed snapshot after
  /// `true`; its values may already reflect additional changes.
  ///
  /// # Errors
  ///
  /// Returns [`Error::Io`] for poll, interrupted waits, invalid timeout ranges,
  /// or descriptor failures, and the same I/O or missing-file errors as an
  /// interface read if acknowledgement fails (including cgroup removal).
  /// See [poll(2)](https://man7.org/linux/man-pages/man2/poll.2.html).
  pub fn wait(&mut self, timeout: Option<Duration>) -> Result<bool, Error> {
    match Readiness::wait(&self.file, PollFlags::PRI, timeout)? {
      | false => Ok(false),
      | true => {
        self.acknowledge()?;
        Ok(true)
      },
    }
  }

  fn acknowledge(&mut self) -> Result<(), Error> {
    self.file.seek(SeekFrom::Start(0)).map_err(|error| Error::read(self.path.clone(), error))?;
    io::copy(&mut self.file, &mut io::sink()).map_err(|error| Error::read(self.path.clone(), error))?;
    Ok(())
  }
}

impl AsFd for EventPoller {
  fn as_fd(&self) -> BorrowedFd<'_> { self.file.as_fd() }
}

/// Coalesced inotify flags from one batch of configuration notifications.
///
/// Contains no file values, timestamps, units, or event count. Multiple flags
/// can be true together; reread configuration after modifications or overflow.
/// See [inotify(7)](https://man7.org/linux/man-pages/man7/inotify.7.html).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChangeNotification {
  modified: bool,
  removed: bool,
  overflowed: bool,
}

impl ChangeNotification {
  /// Whether a write or kernel content notification was observed.
  ///
  /// A write can leave the parsed value unchanged; this is not a value diff.
  pub fn modified(&self) -> bool { self.modified }

  /// Whether the kernel reported removal or watch invalidation, including unmounting.
  ///
  /// The watcher becomes terminal and does not follow a replacement interface.
  /// Cgroup interface removal does not guarantee an inotify event; `false`
  /// does not establish that the interface still exists.
  pub fn removed(&self) -> bool { self.removed }

  /// Whether the kernel inotify queue overflowed and notifications were lost.
  ///
  /// Reread the configuration; notification history is incomplete. A watcher
  /// whose removal event was lost may need to be recreated by the caller.
  pub fn overflowed(&self) -> bool { self.overflowed }

  fn include(&mut self, flags: inotify::ReadFlags) {
    self.modified |= flags.contains(inotify::ReadFlags::MODIFY);
    self.removed |=
      flags.intersects(inotify::ReadFlags::DELETE_SELF | inotify::ReadFlags::IGNORED | inotify::ReadFlags::UNMOUNT);
    self.overflowed |= flags.contains(inotify::ReadFlags::QUEUE_OVERFLOW);
  }
}

/// An inotify subscription to one opened configuration interface inode.
///
/// Create it with [`super::Cgroup::watch_configuration`]. Registration resolves
/// `/proc/self/fd` for an interface opened relative to the cgroup directory;
/// the diagnostic path is never used for registration. The watch follows the
/// inode through renames and does not follow replacements. The temporary
/// interface descriptor is closed after registration so it does not delay
/// deletion notifications. The watch and inotify descriptor close on drop.
///
/// Implements [`AsFd`] for external readable (`POLLIN`/`EPOLLIN`) event loops.
/// Call [`Self::wait`] with zero timeout to consume ready notifications.
/// No baseline value is read, and nothing is written. See
/// [inotify(7)](https://man7.org/linux/man-pages/man7/inotify.7.html).
///
/// Cgroup interfaces use kernfs, whose internal removal does not guarantee
/// inotify deletion events. A watch can remain idle after its interface or
/// cgroup disappears. Use bounded waits and fresh reads when tracking lifecycle;
/// an indefinite wait can remain blocked after removal. See the
/// [Linux kernfs removal path](https://github.com/torvalds/linux/blob/v5.15/fs/kernfs/dir.c#L1236-L1295).
#[derive(Debug)]
pub struct ConfigurationWatcher {
  descriptor: OwnedFd,
  interface: ConfigurationInterface,
  removed: bool,
}

impl ConfigurationWatcher {
  const READ_BUFFER_SIZE: usize = 4096;

  pub(crate) fn open(directory: impl AsFd, path: PathBuf, interface: ConfigurationInterface) -> Result<Self, Error> {
    let file =
      openat(directory, interface.filename(), OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW, Mode::empty())
        .map_err(|error| Error::read(path, error.into()))?;
    let descriptor =
      inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK).map_err(io::Error::from)?;
    let pinned_path = format!("/proc/self/fd/{}", file.as_raw_fd());
    inotify::add_watch(&descriptor, pinned_path, inotify::WatchFlags::MODIFY | inotify::WatchFlags::DELETE_SELF)
      .map_err(io::Error::from)?;
    Ok(Self { descriptor, interface, removed: false })
  }

  /// The configuration interface selected when this watcher was created.
  pub fn interface(&self) -> ConfigurationInterface { self.interface }

  /// Waits for and consumes a batch of configuration notifications.
  ///
  /// `None` waits indefinitely; a duration uses seconds with nanosecond
  /// precision, and zero checks readiness without blocking. Returns `None`
  /// when the timeout expires. Notifications can coalesce; the returned flags
  /// describe a batch, not a count of writes or the current configuration.
  /// Each call consumes at most one read buffer; remaining queued events keep
  /// the descriptor readable for the next call.
  /// After modification or overflow, reread the relevant typed snapshot.
  /// Observed removal is reported once and makes the watcher terminal.
  /// Interface disappearance need not emit an event; see [`Self`] for the
  /// kernfs lifecycle limitation.
  ///
  /// # Errors
  ///
  /// Returns [`Error::Io`] for interrupted waits, invalid timeout ranges,
  /// poll or inotify read failures. Calls after a removal notification return
  /// [`io::ErrorKind::BrokenPipe`]. See [inotify(7)](https://man7.org/linux/man-pages/man7/inotify.7.html).
  pub fn wait(&mut self, timeout: Option<Duration>) -> Result<Option<ChangeNotification>, Error> {
    match self.removed {
      | true => return Err(io::Error::from(rustix::io::Errno::PIPE).into()),
      | false => {},
    }
    match Readiness::wait(&self.descriptor, PollFlags::IN, timeout)? {
      | false => Ok(None),
      | true => {
        let mut buffer = [MaybeUninit::uninit(); Self::READ_BUFFER_SIZE];
        let mut reader = inotify::Reader::new(&self.descriptor, &mut buffer);
        let mut notification = ChangeNotification::default();
        loop {
          match reader.next() {
            | Ok(event) => notification.include(event.events()),
            | Err(rustix::io::Errno::AGAIN) => break,
            | Err(error) => return Err(io::Error::from(error).into()),
          }
          match reader.is_buffer_empty() {
            | true => break,
            | false => {},
          }
        }
        self.removed = notification.removed;
        Ok((notification != ChangeNotification::default()).then_some(notification))
      },
    }
  }
}

impl AsFd for ConfigurationWatcher {
  fn as_fd(&self) -> BorrowedFd<'_> { self.descriptor.as_fd() }
}

struct Readiness;

impl Readiness {
  fn wait(descriptor: impl AsFd, interest: PollFlags, timeout: Option<Duration>) -> Result<bool, Error> {
    let timeout = timeout
      .map(Timespec::try_from)
      .transpose()
      .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut descriptors = [PollFd::new(&descriptor, interest)];
    match poll(&mut descriptors, timeout.as_ref()).map_err(io::Error::from)? {
      | 0 => Ok(false),
      | _ => {
        let flags = descriptors.first().map(PollFd::revents).unwrap_or_else(PollFlags::empty);
        match flags {
          | flags if flags.contains(PollFlags::NVAL) => Err(io::Error::from(rustix::io::Errno::BADF).into()),
          | flags if flags.contains(PollFlags::HUP) => Err(io::Error::from(rustix::io::Errno::PIPE).into()),
          | flags => Ok(flags.intersects(interest | PollFlags::ERR)),
        }
      },
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    fs,
    os::unix::{fs::symlink, net::UnixStream},
    sync::atomic::{AtomicU64, Ordering},
  };

  use assertables::{assert_err, assert_ok};
  use strum::IntoEnumIterator;

  use super::*;

  struct Fixture {
    root: PathBuf,
  }

  impl Fixture {
    fn new() -> Self {
      static NEXT_ID: AtomicU64 = AtomicU64::new(0);
      let root = std::env::temp_dir().join(format!(
        "sakai-notification-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
      ));
      assert_ok!(fs::create_dir(&root));
      assert_ok!(fs::create_dir(root.join("group")));
      Self { root }
    }

    fn directory(&self) -> OwnedFd {
      assert_ok!(rustix::fs::open(
        self.root.join("group"),
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY,
        Mode::empty()
      ))
    }

    fn file(&self, name: &str) -> PathBuf { self.root.join("group").join(name) }
  }

  impl Drop for Fixture {
    fn drop(&mut self) {
      assert_ok!(fs::remove_dir_all(&self.root));
    }
  }

  #[test]
  fn selectors_round_trip_only_supported_filenames() {
    for interface in EventInterface::iter() {
      assert_eq!(assert_ok!(interface.filename().parse::<EventInterface>()), interface);
      assert_eq!(interface.to_string(), interface.filename());
    }
    for interface in ConfigurationInterface::iter() {
      assert_eq!(assert_ok!(interface.filename().parse::<ConfigurationInterface>()), interface);
      assert_eq!(interface.to_string(), interface.filename());
    }
    for invalid in ["cpu.pressure", "memory.pressure", "io.pressure", "../cpu.max", "cpu.stat", "cpu.max\n"] {
      assert_err!(invalid.parse::<EventInterface>());
      assert_err!(invalid.parse::<ConfigurationInterface>());
    }
  }

  #[test]
  fn subscriptions_report_missing_interfaces_and_reject_symlinks() {
    let fixture = Fixture::new();
    let directory = fixture.directory();
    let events = EventInterface::CgroupEvents;
    let config = ConfigurationInterface::CpuMax;
    assert!(
      matches!(EventPoller::open(&directory, fixture.file(events.filename()), events), Err(Error::FileMissing { path }) if path == fixture.file(events.filename()))
    );
    assert!(
      matches!(ConfigurationWatcher::open(&directory, fixture.file(config.filename()), config), Err(Error::FileMissing { path }) if path == fixture.file(config.filename()))
    );
    assert_ok!(symlink("/etc/passwd", fixture.file(events.filename())));
    assert_ok!(symlink("/etc/passwd", fixture.file(config.filename())));
    assert!(matches!(
      EventPoller::open(&directory, fixture.file(events.filename()), events),
      Err(Error::Operation { path, source, .. }) if path == fixture.file(events.filename()) && source.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error())
    ));
    assert!(matches!(
      ConfigurationWatcher::open(&directory, fixture.file(config.filename()), config),
      Err(Error::Operation { path, source, .. }) if path == fixture.file(config.filename()) && source.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error())
    ));
  }

  #[test]
  fn polling_primes_without_parsing_or_changing_interface_contents() {
    let fixture = Fixture::new();
    let interface = EventInterface::CgroupEvents;
    let path = fixture.file(interface.filename());
    let raw = "future_key invalid-number\n";
    assert_ok!(fs::write(&path, raw));
    let mut subscription = assert_ok!(EventPoller::open(fixture.directory(), path.clone(), interface));
    assert_eq!(subscription.interface(), interface);
    assert!(!assert_ok!(subscription.wait(Some(Duration::ZERO))));
    assert_eq!(assert_ok!(fs::read_to_string(path)), raw);
  }

  #[test]
  fn watch_uses_open_directory_after_rename_and_observes_writes_and_removal() {
    let fixture = Fixture::new();
    let interface = ConfigurationInterface::CpuMax;
    let original = fixture.file(interface.filename());
    assert_ok!(fs::write(&original, "max 100000\n"));
    let directory = fixture.directory();
    let renamed = fixture.root.join("renamed");
    assert_ok!(fs::rename(fixture.root.join("group"), &renamed));
    let actual = renamed.join(interface.filename());
    let mut watcher = assert_ok!(ConfigurationWatcher::open(directory, original, interface));
    assert_eq!(watcher.interface(), interface);
    assert_eq!(assert_ok!(watcher.wait(Some(Duration::ZERO))), None);
    assert_ok!(fs::write(&actual, "invalid contents are not parsed\n"));
    assert_eq!(
      assert_ok!(watcher.wait(Some(Duration::from_secs(1)))),
      Some(ChangeNotification { modified: true, removed: false, overflowed: false })
    );
    assert_eq!(assert_ok!(fs::read_to_string(&actual)), "invalid contents are not parsed\n");
    assert_eq!(assert_ok!(watcher.wait(Some(Duration::ZERO))), None);
    assert_ok!(fs::remove_file(actual));
    assert_eq!(
      assert_ok!(watcher.wait(Some(Duration::from_secs(1)))),
      Some(ChangeNotification { modified: false, removed: true, overflowed: false })
    );
    assert!(matches!(watcher.wait(None), Err(Error::Io(error)) if error.kind() == io::ErrorKind::BrokenPipe));
  }

  #[test]
  fn readiness_handles_timeout_pending_input_and_invalid_timeout() {
    let (reader, writer) = assert_ok!(UnixStream::pair());
    assert!(!assert_ok!(Readiness::wait(&reader, PollFlags::IN, Some(Duration::ZERO))));
    assert_ok!(rustix::io::write(&writer, b"ready"));
    assert!(assert_ok!(Readiness::wait(&reader, PollFlags::IN, None)));
    assert!(
      matches!(Readiness::wait(&reader, PollFlags::IN, Some(Duration::MAX)), Err(Error::Io(error)) if error.kind() == io::ErrorKind::InvalidInput)
    );
  }

  #[test]
  fn batch_flags_preserve_modifications_removal_and_queue_overflow() {
    for (flags, expected) in [
      (inotify::ReadFlags::MODIFY, ChangeNotification { modified: true, removed: false, overflowed: false }),
      (inotify::ReadFlags::DELETE_SELF, ChangeNotification { modified: false, removed: true, overflowed: false }),
      (inotify::ReadFlags::IGNORED, ChangeNotification { modified: false, removed: true, overflowed: false }),
      (inotify::ReadFlags::UNMOUNT, ChangeNotification { modified: false, removed: true, overflowed: false }),
      (inotify::ReadFlags::QUEUE_OVERFLOW, ChangeNotification { modified: false, removed: false, overflowed: true }),
      (
        inotify::ReadFlags::MODIFY | inotify::ReadFlags::IGNORED | inotify::ReadFlags::QUEUE_OVERFLOW,
        ChangeNotification { modified: true, removed: true, overflowed: true },
      ),
    ] {
      let mut actual = ChangeNotification::default();
      actual.include(flags);
      assert_eq!(actual, expected);
      assert_eq!(
        (actual.modified(), actual.removed(), actual.overflowed()),
        (expected.modified, expected.removed, expected.overflowed)
      );
    }
  }
}
