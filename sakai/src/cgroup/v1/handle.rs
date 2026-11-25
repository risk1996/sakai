use std::{
  ffi::OsStr,
  os::unix::ffi::OsStrExt,
  path::{Path, PathBuf},
};

use procfs::process::Process;
use rustix::fs;

use super::{ReadableFile, Version, WritableFile};
use crate::{
  Error,
  cgroup::common::{
    directory::Directory,
    discovery::{open_candidates, processes},
  },
};

// Linux UAPI linux/magic.h.
const CGROUP_SUPER_MAGIC: fs::FsWord = 0x0027_e0eb;

/// An open directory in a single v1 hierarchy.
///
/// A handle pins its directory and does not follow subsequent process
/// migrations. Each interface read observes a fresh snapshot. Different
/// controllers may require separate handles.
/// See the [Linux cgroup v1 guide](https://docs.kernel.org/admin-guide/cgroup-v1/cgroups.html).
#[derive(Debug)]
#[non_exhaustive]
pub struct Cgroup {
  directory: Directory,
  cpuset_noprefix: bool,
}

impl Cgroup {
  /// Opens the current process's cgroup in a v1 controller or named hierarchy.
  ///
  /// # Errors
  ///
  /// Returns the same discovery and open errors as [Self::from_pid].
  pub fn from_current_process(controller: &str) -> Result<Self, Error> {
    Self::from_pid(std::process::id(), controller)
  }

  /// Opens a process's cgroup for a controller such as cpu or a named hierarchy
  /// such as name=systemd. Mounts are resolved in the caller's namespace.
  ///
  /// The most specific matching mount root is attempted first, including bind
  /// mounts. The process can move or exit during discovery.
  ///
  /// # Errors
  ///
  /// Returns [Error::V1MembershipMissing] when the process has no such
  /// membership, [Error::V1MountMissing] when no mount exposes it, or
  /// [Error::DeletedCgroup] when procfs reports removal. An invalid PID
  /// returns [Error::Io]. Procfs and open failures retain their source.
  pub fn from_pid(pid: u32, controller: &str) -> Result<Self, Error> {
    let (target, caller) = processes(pid)?;
    open_candidates(
      candidates(&target, &caller, controller)?,
      |candidate| {
        let cgroup = Self::from_path(&candidate.path)?;
        cgroup.verify_device(candidate.device)?;
        Ok(cgroup)
      },
      || Error::V1MountMissing { controller: controller.into() },
    )
  }

  /// Opens an explicit directory and verifies that it is on a v1 cgroup filesystem.
  ///
  /// # Errors
  ///
  /// Symlinks and parent traversal are rejected. A different filesystem
  /// returns [Error::NotCgroupV1]. Invalid input and OS failures retain their cause.
  pub fn from_path(path: &Path) -> Result<Self, Error> {
    let directory = Directory::open(path)?;
    directory.verify(CGROUP_SUPER_MAGIC, Error::NotCgroupV1)?;
    // The v1 cpuset-only noprefix mount exposes the regular file "cpus".
    // A child directory with the same name does not imply a noprefix mount.
    let cpuset_noprefix = fs::statat(&directory.fd, "cpus", fs::AtFlags::SYMLINK_NOFOLLOW)
      .is_ok_and(|stat| fs::FileType::from_raw_mode(stat.st_mode) == fs::FileType::RegularFile);
    Ok(Self { directory, cpuset_noprefix })
  }

  /// Returns the diagnostic path used to open the pinned directory.
  ///
  /// This path can become stale after a rename or removal.
  #[must_use]
  pub fn path(&self) -> &Path { &self.directory.path }

  /// Opens a child by a single directory name within this hierarchy.
  ///
  /// # Errors
  ///
  /// Invalid names produce [Error::Io]. Mounts from another hierarchy
  /// produce [Error::NotCgroupV1]. Other failures retain their OS cause.
  pub fn child(&self, name: &OsStr) -> Result<Self, Error> {
    let child = Self { directory: self.directory.child(name)?, cpuset_noprefix: self.cpuset_noprefix };
    child.verify_device(self.directory.device()?)?;
    Ok(child)
  }

  /// Lists child cgroups. Children removed during enumeration are omitted.
  ///
  /// # Errors
  ///
  /// Enumeration and child-open failures retain their OS cause. A child on a
  /// different filesystem returns [Error::NotCgroupV1].
  pub fn children(&self) -> Result<Vec<Self>, Error> { self.directory.children(|name| self.child(name)) }

  /// Reads a typed snapshot from a fixed interface filename.
  ///
  /// # Errors
  ///
  /// See [Self::read_file].
  pub fn read_value<F: ReadableFile<Version = Version, Key = ()>>(&self) -> Result<F::Value, Error> {
    self.read_file::<F>(&())
  }

  /// Reads a fresh snapshot from the filename determined by the interface key.
  ///
  /// # Errors
  ///
  /// An absent interface returns [Error::FileMissing], unsupported operations
  /// return [Error::NotSupported], and invalid contents return [Error::Parse].
  /// Other OS failures retain their diagnostic path and cause.
  pub fn read_file<F: ReadableFile<Version = Version>>(&self, key: &F::Key) -> Result<F::Value, Error> {
    self.directory.read::<F>(self.interface_name(&F::filename(key)))
  }

  /// Writes one typed command to a fixed interface filename.
  ///
  /// # Errors
  ///
  /// See [Self::write_file].
  pub fn write_value<F: WritableFile<Version = Version, Key = ()>>(
    &self,
    command: F::Command<'_>,
  ) -> Result<(), Error> {
    self.write_file::<F>(&(), command)
  }

  /// Encodes and writes one command to the filename determined by the key.
  ///
  /// # Errors
  ///
  /// Returns [Error::Encode] for unrepresentable commands,
  /// [Error::FileMissing] for absent interfaces, [Error::NotSupported] for
  /// unsupported operations, or [Error::Operation] for kernel write failures.
  pub fn write_file<F: WritableFile<Version = Version>>(
    &self,
    key: &F::Key,
    command: F::Command<'_>,
  ) -> Result<(), Error> {
    self.directory.write::<F>(self.interface_name(&F::filename(key)), command)
  }

  fn interface_name<'a>(&self, filename: &'a str) -> &'a str {
    if self.cpuset_noprefix { filename.strip_prefix("cpuset.").unwrap_or(filename) } else { filename }
  }

  fn verify_device(&self, device: fs::Dev) -> Result<(), Error> {
    if self.directory.device()? == device { Ok(()) } else { Err(Error::NotCgroupV1) }
  }
}

#[derive(Debug, PartialEq, Eq)]
struct Candidate {
  path: PathBuf,
  device: fs::Dev,
}

fn candidates(target: &Process, caller: &Process, controller: &str) -> Result<Vec<Candidate>, Error> {
  let membership = target
    .cgroups()
    .map_err(Error::procfs)?
    .0
    .into_iter()
    .find(|entry| entry.hierarchy != 0 && entry.controllers.iter().any(|name| name == controller))
    .ok_or_else(|| Error::V1MembershipMissing { controller: controller.into() })?;
  if let Some(path) = membership.pathname.strip_suffix(" (deleted)") {
    return Err(Error::DeletedCgroup { path: path.into() });
  }
  let path = Path::new(&membership.pathname);
  let mut mounts: Vec<_> = caller
    .mountinfo()
    .map_err(Error::procfs)?
    .0
    .into_iter()
    .filter(|mount| mount.fs_type == "cgroup")
    .filter(|mount| {
      membership.controllers.iter().all(|entry| match entry.split_once('=') {
        | Some((key, value)) => mount.super_options.get(key).and_then(|option| option.as_deref()) == Some(value),
        | None => mount.super_options.get(entry).is_some_and(Option::is_none),
      })
    })
    .filter_map(|mount| {
      let root = mount_path(mount.root.as_ref());
      let relative = path.strip_prefix(&root).ok()?;
      let (major, minor) = mount.majmin.split_once(':')?;
      Some((root.components().count(), Candidate {
        path: mount_path(mount.mount_point.as_os_str()).join(relative),
        device: fs::makedev(major.parse().ok()?, minor.parse().ok()?),
      }))
    })
    .collect();
  mounts.sort_by_key(|(depth, _)| std::cmp::Reverse(*depth));
  if mounts.is_empty() {
    return Err(Error::V1MountMissing { controller: controller.into() });
  }
  Ok(mounts.into_iter().map(|(_, candidate)| candidate).collect())
}

// mountinfo quotes space, tab, newline and backslash as octal byte escapes.
fn mount_path(path: &OsStr) -> PathBuf {
  let mut bytes = path.as_bytes();
  let mut decoded = Vec::with_capacity(bytes.len());
  while let [first, remaining @ ..] = bytes {
    let (byte, rest) = match bytes {
      | [b'\\', b'0', b'4', b'0', rest @ ..] => (b' ', rest),
      | [b'\\', b'0', b'1', b'1', rest @ ..] => (b'\t', rest),
      | [b'\\', b'0', b'1', b'2', rest @ ..] => (b'\n', rest),
      | [b'\\', b'1', b'3', b'4', rest @ ..] => (b'\\', rest),
      | _ => (*first, remaining),
    };
    decoded.push(byte);
    bytes = rest;
  }
  PathBuf::from(OsStr::from_bytes(&decoded))
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;
  use indoc::indoc;

  use super::*;

  #[test]
  fn resolves_hybrid_memberships_and_orders_only_matching_mounts() {
    let target_dir = assert_ok!(tempfile::tempdir());
    let caller_dir = assert_ok!(tempfile::tempdir());
    let target = target_dir.path().join("123");
    let caller = caller_dir.path().join("456");
    assert_ok!(std::fs::create_dir(&target));
    assert_ok!(std::fs::create_dir(&caller));
    assert_ok!(std::fs::write(target.join("cgroup"), indoc! {"
      0::/unified
      2:cpu,cpuacct:/jobs/team A/run:1
      3:memory:/memory/job
      4:name=systemd:/init.scope
      5:pids:/removed (deleted)
      6:blkio:/not-mounted
    "}));
    assert_ok!(std::fs::write(caller.join("mountinfo"), indoc! {r"
      1 0 0:1 / / rw - tmpfs rootfs rw
      2 1 0:2 / /sys/fs/cgroup/unified rw - cgroup2 cgroup rw
      3 1 0:3 / /v1/combined rw - cgroup cpu,cpuacct rw,cpuacct,cpu
      4 1 0:3 /jobs/team\040A /delegated\040cpu rw - cgroup cgroup rw,cpu,cpuacct
      5 1 0:4 / /v1/memory rw - cgroup cgroup rw,memory
      6 1 0:5 / /v1/systemd rw - cgroup cgroup rw,name=systemd
      7 1 0:6 / /wrong rw - cgroup cpu rw,name=cpu
      8 1 0:3 /elsewhere /hidden rw - cgroup cgroup rw,cpu,cpuacct
    "}));
    let target = assert_ok!(Process::new_with_root(target));
    let caller = assert_ok!(Process::new_with_root(caller));

    let expected = [Candidate { path: "/delegated cpu/run:1".into(), device: fs::makedev(0, 3) }, Candidate {
      path: "/v1/combined/jobs/team A/run:1".into(),
      device: fs::makedev(0, 3),
    }];
    for controller in ["cpu", "cpuacct"] {
      assert_eq!(assert_ok!(candidates(&target, &caller, controller)), expected);
    }
    assert_eq!(assert_ok!(candidates(&target, &caller, "memory")), [Candidate {
      path: "/v1/memory/memory/job".into(),
      device: fs::makedev(0, 4)
    },]);
    assert_eq!(assert_ok!(candidates(&target, &caller, "name=systemd")), [Candidate {
      path: "/v1/systemd/init.scope".into(),
      device: fs::makedev(0, 5)
    },]);
    assert!(
      matches!(candidates(&target, &caller, "pids"), Err(Error::DeletedCgroup { path }) if path == Path::new("/removed"))
    );
    assert!(matches!(candidates(&target, &caller, "blkio"), Err(Error::V1MountMissing { .. })));
    assert!(matches!(candidates(&target, &caller, "cpuset"), Err(Error::V1MembershipMissing { .. })));
  }
}
