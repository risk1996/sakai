use std::{ffi::OsStr, path::Path};

use rustix::fs;

use super::{
  notification::{ConfigurationInterface, ConfigurationWatcher, EventInterface, EventPoller},
  path::{CgroupPath, CgroupPathError},
};
use crate::{
  cgroup::{
    directory::Directory,
    discovery::{open_candidates, processes},
  },
  error::Error,
  v2::{ReadableFile, WritableFile},
};

// Linux UAPI linux/magic.h; rustix does not export this constant.
const CGROUP2_SUPER_MAGIC: fs::FsWord = 0x6367_7270;

/// An open cgroup v2 directory. Reads use its descriptor, not its display path.
///
/// Each read opens a fresh, read-only interface descriptor. Reads across files
/// are separate snapshots. Explicit setters write one command at a time.
///
/// See [cgroups(7)](https://man7.org/linux/man-pages/man7/cgroups.7.html),
/// [proc_pid_cgroup(5)](https://man7.org/linux/man-pages/man5/proc_pid_cgroup.5.html), and
/// [proc_pid_mountinfo(5)](https://man7.org/linux/man-pages/man5/proc_pid_mountinfo.5.html).
#[derive(Debug)]
#[non_exhaustive]
pub struct Cgroup {
  directory: Directory,
}

impl Cgroup {
  /// Discovers this process's cgroup through its visible cgroup2 mounts.
  ///
  /// See [cgroups(7)](https://man7.org/linux/man-pages/man7/cgroups.7.html),
  /// [proc_pid_cgroup(5)](https://man7.org/linux/man-pages/man5/proc_pid_cgroup.5.html), and
  /// [proc_pid_mountinfo(5)](https://man7.org/linux/man-pages/man5/proc_pid_mountinfo.5.html).
  ///
  /// # Errors
  ///
  /// Returns the same discovery and open errors as [`Self::from_pid`].
  pub fn from_current_process() -> Result<Self, Error> { Self::from_pid(std::process::id()) }

  /// Resolves a process's membership using the caller's mount namespace.
  ///
  /// See [cgroups(7)](https://man7.org/linux/man-pages/man7/cgroups.7.html),
  /// [proc_pid_cgroup(5)](https://man7.org/linux/man-pages/man5/proc_pid_cgroup.5.html), and
  /// [proc_pid_mountinfo(5)](https://man7.org/linux/man-pages/man5/proc_pid_mountinfo.5.html).
  ///
  /// The target's membership is resolved against mounts visible to the caller.
  /// A process can move cgroups or exit during discovery; this handle keeps the
  /// directory it opened and does not follow later migrations.
  ///
  /// # Errors
  ///
  /// A zero PID or a PID larger than `i32::MAX` produces [`Error::Io`] with
  /// [`std::io::ErrorKind::InvalidInput`]. Missing unified membership or a visible
  /// mount produces [`Error::NotCgroupV2`]. Deleted membership produces
  /// [`Error::DeletedCgroup`]. Procfs and directory-open failures retain their
  /// causes in [`Error::Operation`] when a path is known, or [`Error::Io`]
  /// otherwise. Native procfs errno is preserved where available; procfs's
  /// normalized missing and permission categories use ENOENT and EACCES.
  /// Discovery tries matching mounts from the most specific root first. If
  /// every candidate fails, the first candidate's error is returned.
  pub fn from_pid(pid: u32) -> Result<Self, Error> {
    let (process, caller) = processes(pid)?;
    let candidates = CgroupPath::candidates(&process, &caller).map_err(|error| match error {
      | CgroupPathError::Procfs(source) => Error::procfs(source),
      | CgroupPathError::DeletedCgroup { path } => Error::DeletedCgroup { path },
      | CgroupPathError::UnifiedHierarchyNotFound | CgroupPathError::UnifiedMountNotFound => Error::NotCgroupV2,
    })?;
    open_candidates(candidates, |candidate| Self::from_path(candidate.as_ref()), || Error::NotCgroupV2)
  }

  /// Opens a directory and verifies cgroup2 filesystem magic.
  /// Symlinks and parent traversal are rejected, including on old kernels.
  ///
  /// See [openat2(2)](https://man7.org/linux/man-pages/man2/openat2.2.html) for path-resolution flags
  /// and [statfs(2)](https://man7.org/linux/man-pages/man2/statfs.2.html) for filesystem identification.
  ///
  /// # Errors
  ///
  /// Invalid names or parent traversal produce [`crate::Error::Io`] with
  /// [`std::io::ErrorKind::InvalidInput`]. A directory on another filesystem
  /// produces [`crate::Error::NotCgroupV2`]; permission and open failures retain
  /// their path and operating-system error in [`Error::Operation`].
  pub fn from_path(path: &Path) -> Result<Self, Error> {
    let directory = Directory::open(path)?;
    directory.verify(CGROUP2_SUPER_MAGIC, Error::NotCgroupV2)?;
    Ok(Self { directory })
  }

  /// Opens one child by name; names containing slashes or traversal are invalid.
  ///
  /// See [openat2(2)](https://man7.org/linux/man-pages/man2/openat2.2.html) for path-resolution flags
  /// and [statfs(2)](https://man7.org/linux/man-pages/man2/statfs.2.html) for filesystem identification.
  ///
  /// # Errors
  ///
  /// Invalid names or parent traversal produce [`crate::Error::Io`] with
  /// [`std::io::ErrorKind::InvalidInput`]. A directory on another filesystem
  /// produces [`crate::Error::NotCgroupV2`]; permission and open failures retain
  /// their path and operating-system error in [`Error::Operation`].
  pub fn child(&self, name: &OsStr) -> Result<Self, Error> {
    let directory = self.directory.child(name)?;
    directory.verify(CGROUP2_SUPER_MAGIC, Error::NotCgroupV2)?;
    Ok(Self { directory })
  }

  /// Lists directories, including child names that resemble interface files.
  /// A child removed during enumeration is omitted.
  ///
  /// Enumeration is a separate observation for each child, not an atomic tree
  /// snapshot. See [getdents(2)](https://man7.org/linux/man-pages/man2/getdents.2.html).
  ///
  /// # Errors
  ///
  /// Returns [`Error::Operation`] with the directory path and source for
  /// enumeration or child-open failures other than a removed child.
  pub fn children(&self) -> Result<Vec<Self>, Error> { self.directory.children(|name| self.child(name)) }

  /// The path used to open the handle; it is diagnostic and may become stale.
  ///
  /// Renaming or deleting the cgroup can make this path stale. Interface reads
  /// continue to use the open directory descriptor; see [openat(2)](https://man7.org/linux/man-pages/man2/openat.2.html).
  pub fn path(&self) -> &Path { &self.directory.path }

  /// Opens a read-only subscription to one evented cgroup interface.
  ///
  /// An initial read establishes the notification baseline. Later calls to
  /// [`EventPoller::wait`] acknowledge changes on that same descriptor without
  /// changing this handle's fresh-read semantics. Notifications can coalesce;
  /// read the corresponding typed snapshot after each notification.
  /// Pressure interfaces require writable trigger registration and are excluded.
  /// See [cgroups(7)](https://man7.org/linux/man-pages/man7/cgroups.7.html).
  ///
  /// # Errors
  ///
  /// Returns [`Error::FileMissing`] for an absent interface, [`Error::NotSupported`]
  /// for an unsupported operation, or [`Error::Operation`] for other open or
  /// initial-read failures, preserving the interface path and underlying cause.
  /// Contents are acknowledged without parsing them.
  pub fn poll_events(&self, interface: EventInterface) -> Result<EventPoller, Error> {
    EventPoller::open(&self.directory.fd, self.path().join(interface.filename()), interface)
  }

  /// Watches writes to one configuration interface using inotify.
  ///
  /// The watch follows the opened interface inode, including after cgroup
  /// renames; it does not reopen the diagnostic path or follow replacement files.
  /// Registration requires a mounted `/proc` to resolve the opened descriptor.
  /// Notifications can coalesce and do not contain values; read the corresponding
  /// typed snapshot after a change. See [inotify(7)](https://man7.org/linux/man-pages/man7/inotify.7.html).
  ///
  /// # Errors
  ///
  /// Returns [`Error::FileMissing`] for an absent interface, [`Error::NotSupported`]
  /// for an unsupported operation, or [`Error::Operation`] for other interface-open
  /// failures. Returns [`Error::Io`] for procfs, inotify resource-limit, or other
  /// watch-registration failures.
  pub fn watch_configuration(&self, interface: ConfigurationInterface) -> Result<ConfigurationWatcher, Error> {
    ConfigurationWatcher::open(&self.directory.fd, self.path().join(interface.filename()), interface)
  }

  /// Reads a typed cgroup value from a fixed filename.
  ///
  /// # Errors
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn read_value<F: ReadableFile<Version = super::Version, Key = ()>>(&self) -> Result<F::Value, Error> {
    self.read_file::<F>(&())
  }

  /// Reads a typed cgroup value, resolving the filename with the supplied key.
  ///
  /// # Errors
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn read_file<F: ReadableFile<Version = super::Version>>(&self, key: &F::Key) -> Result<F::Value, Error> {
    self.directory.read::<F>(&F::filename(key))
  }

  /// Writes one typed command to a fixed filename.
  ///
  /// Read-only declarations cannot be used as write targets.
  ///
  /// ```compile_fail
  /// use sakai::{Cgroup, v2::cpu::CpuStat};
  ///
  /// fn write_stats(cgroup: &Cgroup) {
  ///   let _ = cgroup.write_value::<CpuStat>(());
  /// }
  /// ```
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn write_value<F: WritableFile<Version = super::Version, Key = ()>>(
    &self,
    command: F::Command<'_>,
  ) -> Result<(), Error> {
    self.write_file::<F>(&(), command)
  }

  /// Writes one typed command, resolving the filename with the supplied key.
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn write_file<F: WritableFile<Version = super::Version>>(
    &self,
    key: &F::Key,
    command: F::Command<'_>,
  ) -> Result<(), Error> {
    self.directory.write::<F>(&F::filename(key), command)
  }
}

#[cfg(test)]
mod tests {
  use std::{error::Error as _, fmt::Debug, io, path::PathBuf, str::FromStr};

  use assertables::{assert_err, assert_ok};
  use procfs::process::Process;
  use rustix::fs::{Mode, OFlags};

  use super::*;
  use crate::{
    error::{ParseError, ParseValueError},
    v2::{
      CgroupFile,
      core::{CgroupEvents, CgroupStat, CgroupType},
      cpu::{CpuIdle, CpuStatLocal},
      cpuset::{CpuList, CpusEffective, CpusetPartition, MemoryNodeList, MemsEffective},
      io::{IoLatency, IoMax, IoPressure, IoStat, IoWeight},
      memory::{MemoryCurrent, MemoryHigh, MemoryLow, MemoryMax, MemoryMin, MemoryPeak, MemoryPressure, MemoryStat},
      pids::{PidsCurrent, PidsEvents, PidsMax},
    },
  };

  const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY.union(OFlags::DIRECTORY);

  impl Cgroup {
    // Internal fixtures only: public constructors always verify filesystem magic.
    pub(crate) fn fixture(relative: &str) -> Self {
      let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cgroup/v2").join(relative);
      Self { directory: assert_ok!(Directory::open(&path)) }
    }

    pub(crate) fn check_fixture<T>(&self, name: &str, optional: bool, read: impl FnOnce() -> Result<T, Error>)
    where
      T: FromStr + Debug + PartialEq,
      T::Err: Debug, {
      let path = self.path().join(name);
      match std::fs::read_to_string(&path) {
        | Ok(raw) => {
          assert_eq!(assert_ok!(read()), assert_ok!(raw.parse::<T>()), "path: {path:?}");
          assert_eq!(assert_ok!(std::fs::read_to_string(&path)), raw);
        },
        | Err(error) if optional && error.kind() == io::ErrorKind::NotFound => {
          assert!(matches!(read(), Err(Error::FileMissing { path: missing }) if missing == path));
        },
        | Err(error) => panic!("fixture read failed at {path:?}: {error}"),
      }
    }
  }

  #[test]
  fn parse_failure_keeps_path_and_typed_source() {
    let scratch = assert_ok!(tempfile::tempdir());
    let path = scratch.path().to_path_buf();
    assert_ok!(std::fs::write(path.join(CpuIdle::FILE_NAME), "invalid"));
    let cgroup = Cgroup { directory: assert_ok!(Directory::open(&path)) };
    let error = assert_err!(cgroup.read_value::<CpuIdle>());
    let Error::Parse { path: actual, source } = &error else {
      panic!("expected parse failure, got {error:?}");
    };
    assert_eq!(actual, &cgroup.path().join(CpuIdle::FILE_NAME));
    assert_eq!(error.source().map(ToString::to_string), Some(source.to_string()));
    assert!(matches!(source, ParseError::Invalid { field: "idle", .. }));
    assert!(source.source().is_some());
    assert!(error.to_string().contains("failed to parse"));
    assert!(error.to_string().contains("cgroup content"));
  }

  #[test]
  fn cgroup_type_failure_keeps_raw_input_and_strum_source() {
    let cgroup = Cgroup::fixture("fixtures/invalid_type");
    let error = assert_err!(cgroup.read_value::<crate::v2::core::CgroupType>());
    let Error::Parse { path, source: ParseError::Invalid { raw, field, value, source } } = &error else {
      panic!("expected an invalid field, got {error:?}");
    };
    assert_eq!(path, &cgroup.path().join(CgroupType::FILE_NAME));
    assert_eq!(*field, CgroupType::FILE_NAME);
    assert_eq!(raw.as_ref(), "unknown\n");
    assert_eq!(value.as_ref(), "unknown");
    assert!(matches!(source, ParseValueError::UnknownVariant(strum::ParseError::VariantNotFound)));
  }

  #[test]
  fn missing_interfaces_keep_their_path() {
    let cgroup = Cgroup::fixture("");
    let path = cgroup.path();
    for (result, name) in [
      (cgroup.read_value::<crate::v2::cpu::CpuStatLocal>().map(|_| ()), CpuStatLocal::FILE_NAME),
      (cgroup.read_value::<crate::v2::cpuset::CpuList>().map(|_| ()), CpuList::FILE_NAME),
      (cgroup.read_value::<crate::v2::cpuset::CpusEffective>().map(|_| ()), CpusEffective::FILE_NAME),
      (cgroup.read_value::<crate::v2::cpuset::MemoryNodeList>().map(|_| ()), MemoryNodeList::FILE_NAME),
      (cgroup.read_value::<crate::v2::cpuset::MemsEffective>().map(|_| ()), MemsEffective::FILE_NAME),
      (cgroup.read_value::<crate::v2::cpuset::CpusetPartition>().map(|_| ()), CpusetPartition::FILE_NAME),
      (cgroup.read_value::<crate::v2::io::IoStat>().map(|_| ()), IoStat::FILE_NAME),
      (cgroup.read_value::<crate::v2::io::IoPressure>().map(|_| ()), IoPressure::FILE_NAME),
      (cgroup.read_value::<crate::v2::io::IoWeight>().map(|_| ()), IoWeight::FILE_NAME),
      (cgroup.read_value::<crate::v2::io::IoMax>().map(|_| ()), IoMax::FILE_NAME),
      (cgroup.read_value::<crate::v2::io::IoLatency>().map(|_| ()), IoLatency::FILE_NAME),
      (cgroup.read_value::<crate::v2::core::CgroupType>().map(|_| ()), CgroupType::FILE_NAME),
      (cgroup.read_value::<crate::v2::core::CgroupEvents>().map(|_| ()), CgroupEvents::FILE_NAME),
      (cgroup.read_value::<crate::v2::core::CgroupStat>().map(|_| ()), CgroupStat::FILE_NAME),
      (cgroup.read_value::<crate::v2::pids::PidsCurrent>().map(|_| ()), PidsCurrent::FILE_NAME),
      (cgroup.read_value::<crate::v2::pids::PidsMax>().map(|_| ()), PidsMax::FILE_NAME),
      (cgroup.read_value::<crate::v2::pids::PidsEvents>().map(|_| ()), PidsEvents::FILE_NAME),
      (cgroup.read_value::<crate::v2::memory::MemoryCurrent>().map(|_| ()), MemoryCurrent::FILE_NAME),
      (cgroup.read_value::<crate::v2::memory::MemoryMax>().map(|_| ()), MemoryMax::FILE_NAME),
      (cgroup.read_value::<crate::v2::memory::MemoryHigh>().map(|_| ()), MemoryHigh::FILE_NAME),
      (cgroup.read_value::<crate::v2::memory::MemoryLow>().map(|_| ()), MemoryLow::FILE_NAME),
      (cgroup.read_value::<crate::v2::memory::MemoryMin>().map(|_| ()), MemoryMin::FILE_NAME),
      (cgroup.read_value::<crate::v2::memory::MemoryPeak>().map(|_| ()), MemoryPeak::FILE_NAME),
      (cgroup.read_value::<crate::v2::memory::MemoryStat>().map(|_| ()), MemoryStat::FILE_NAME),
      (cgroup.read_value::<crate::v2::memory::MemoryPressure>().map(|_| ()), MemoryPressure::FILE_NAME),
    ] {
      assert!(matches!(result, Err(Error::FileMissing { path: missing }) if missing == path.join(name)));
    }
    for name in ["", ".", "..", "../cpu", "/cpu", "cpu/stat"] {
      assert!(
        matches!(cgroup.child(OsStr::new(name)), Err(Error::Io(error)) if error.kind() == io::ErrorKind::InvalidInput)
      );
    }
    for pid in [0, u32::MAX] {
      assert!(matches!(Cgroup::from_pid(pid), Err(Error::Io(error)) if error.kind() == io::ErrorKind::InvalidInput));
    }
  }

  #[test]
  fn reads_placement_fixtures_and_root_exemptions() {
    for fixture in ["p4", "p4_older", "p4_root"] {
      let cgroup = Cgroup::fixture(&format!("fixtures/{fixture}"));
      let path = cgroup.path();
      cgroup.check_fixture(CpuList::FILE_NAME, true, || cgroup.read_value::<crate::v2::cpuset::CpuList>());
      cgroup.check_fixture(CpusEffective::FILE_NAME, true, || cgroup.read_value::<crate::v2::cpuset::CpusEffective>());
      cgroup
        .check_fixture(MemoryNodeList::FILE_NAME, true, || cgroup.read_value::<crate::v2::cpuset::MemoryNodeList>());
      cgroup.check_fixture(MemsEffective::FILE_NAME, true, || cgroup.read_value::<crate::v2::cpuset::MemsEffective>());
      match fixture {
        | "p4_root" => assert!(
          matches!(cgroup.read_value::<crate::v2::cpuset::CpusetPartition>(), Err(Error::FileMissing { path: missing }) if missing == path.join(CpusetPartition::FILE_NAME))
        ),
        | _ => cgroup.check_fixture(CpusetPartition::FILE_NAME, false, || {
          cgroup.read_value::<crate::v2::cpuset::CpusetPartition>()
        }),
      }
    }
  }

  #[test]
  fn reads_io_fixtures_without_changing_configuration() {
    for fixture in ["p3", "p3_older"] {
      let cgroup = Cgroup::fixture(&format!("fixtures/{fixture}"));
      let path = cgroup.path();
      cgroup.check_fixture(IoStat::FILE_NAME, false, || cgroup.read_value::<crate::v2::io::IoStat>());
      cgroup.check_fixture(IoPressure::FILE_NAME, false, || cgroup.read_value::<crate::v2::io::IoPressure>());
      cgroup.check_fixture(IoWeight::FILE_NAME, false, || cgroup.read_value::<crate::v2::io::IoWeight>());
      cgroup.check_fixture(IoMax::FILE_NAME, false, || cgroup.read_value::<crate::v2::io::IoMax>());
      match fixture {
        | "p3" => cgroup.check_fixture(IoLatency::FILE_NAME, false, || cgroup.read_value::<crate::v2::io::IoLatency>()),
        | _ => assert!(
          matches!(cgroup.read_value::<crate::v2::io::IoLatency>(), Err(Error::FileMissing { path: missing }) if missing == path.join(IoLatency::FILE_NAME))
        ),
      }
    }
  }

  #[test]
  fn reads_process_and_state_fixtures() {
    for (fixture, frozen, has_subsystems) in [("p2", Some(false), true), ("p2_older", None, false)] {
      let cgroup = Cgroup::fixture(&format!("fixtures/{fixture}"));
      let current = assert_ok!(cgroup.read_value::<crate::v2::pids::PidsCurrent>());
      let max = assert_ok!(cgroup.read_value::<crate::v2::pids::PidsMax>());
      let events = assert_ok!(cgroup.read_value::<crate::v2::pids::PidsEvents>());
      let state = assert_ok!(cgroup.read_value::<crate::v2::core::CgroupEvents>());
      let stat = assert_ok!(cgroup.read_value::<crate::v2::core::CgroupStat>());
      assert_eq!(current, assert_ok!("12".parse::<PidsCurrent>()));
      assert_eq!(max, assert_ok!("10".parse::<PidsMax>()));
      assert_eq!(events, assert_ok!("max 3".parse::<PidsEvents>()));
      assert_eq!(events.max.value, 3);
      assert!(state.populated);
      assert_eq!(state.frozen, frozen);
      assert_eq!(stat.descendants.value, 2);
      assert_eq!(stat.dying_descendants.value, 1);
      assert_eq!(!stat.subsystems.is_empty(), has_subsystems);
      assert_eq!(!stat.dying_subsystems.is_empty(), has_subsystems);
    }
  }

  #[test]
  fn rejects_traversal_and_symlinks_with_both_backends() {
    let root = assert_ok!(fs::open("/", DIRECTORY_FLAGS, Mode::empty()));
    // /proc/self is a kernel-provided symlink, requiring no mutable fixture.
    assert_err!(Directory::open_directory(&root, Path::new("proc/self")));
    assert_err!(Directory::open_components(&root, Path::new("proc/self")));
    assert_err!(Directory::open_directory(&root, Path::new("../")));
    let proc = assert_ok!(Directory::open_components(&root, Path::new("proc")));
    assert_eq!(assert_ok!(fs::fstatfs(proc)).f_type, fs::PROC_SUPER_MAGIC);
  }

  #[test]
  fn tries_next_matching_mount_after_open_failure() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cgroup/v2/fixtures/multiple_mounts/1");
    let process = assert_ok!(Process::new_with_root(root));
    let candidates = assert_ok!(CgroupPath::candidates(&process, &process));
    let mut attempted = Vec::new();
    let selected = assert_ok!(open_candidates(
      candidates,
      |candidate| {
        let path = candidate.as_ref();
        attempted.push(path.to_owned());
        match attempted.len() {
          | 1 => Err(Error::NotCgroupV2),
          | _ => Ok(path.to_owned()),
        }
      },
      || Error::NotCgroupV2
    ));
    assert_eq!(selected, Path::new("/run/parent/pod-1/process.scope"));
    assert_eq!(attempted, [
      PathBuf::from("/run/delegated/process.scope"),
      PathBuf::from("/run/parent/pod-1/process.scope"),
    ]);
  }

  #[test]
  fn reports_first_candidate_failure_when_all_mounts_fail() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cgroup/v2/fixtures/multiple_mounts/1");
    let process = assert_ok!(Process::new_with_root(root));
    let candidates = assert_ok!(CgroupPath::candidates(&process, &process));
    let mut attempted = Vec::new();
    let error = assert_err!(open_candidates(
      candidates,
      |candidate| -> Result<(), Error> {
        let path = candidate.as_ref();
        attempted.push(path.to_owned());
        Err(Error::Operation {
          operation: "open cgroup",
          path: path.to_owned(),
          source: rustix::io::Errno::ACCESS.into(),
        })
      },
      || Error::NotCgroupV2
    ));
    assert_eq!(attempted.len(), 3);
    assert!(
      matches!(error, Error::Operation { path, source, .. } if path == Path::new("/run/delegated/process.scope") && source.raw_os_error() == Some(rustix::io::Errno::ACCESS.raw_os_error()))
    );
    assert!(matches!(
      open_candidates([] as [CgroupPath; 0], |_| Ok(()), || Error::NotCgroupV2),
      Err(Error::NotCgroupV2)
    ));
  }
}
