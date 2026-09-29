use std::{
  ffi::OsStr,
  io,
  os::unix::ffi::OsStrExt,
  path::{Component, Path, PathBuf},
  str::FromStr,
};

use procfs::process::Process;
use rustix::{
  fd::{AsFd, OwnedFd},
  fs::{self, Dir, FileType, Mode, OFlags, ResolveFlags},
};

use crate::cgroup::common::error::Error;

const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
  .union(OFlags::CLOEXEC)
  .union(OFlags::NOFOLLOW)
  .union(OFlags::DIRECTORY);

// Linux UAPI linux/magic.h; rustix does not export this constant.
const CGROUP2_SUPER_MAGIC: fs::FsWord = 0x6367_7270;

/// An open cgroup v2 directory. Reads use its descriptor, not its display path.
///
/// Each read opens a fresh, read-only interface descriptor. Reads across files
/// are separate snapshots, not a transaction. No controller is enabled or changed.
#[derive(Debug)]
pub struct Cgroup {
  directory: OwnedFd,
  path: PathBuf,
}

impl Cgroup {
  /// Discovers this process's cgroup through its visible cgroup2 mounts.
  pub fn from_current_process() -> Result<Self, Error> {
    Self::from_pid(std::process::id())
  }

  /// Resolves a process's membership using the caller's mount namespace.
  pub fn from_pid(pid: u32) -> Result<Self, Error> {
    let pid =
      i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| {
          io::Error::new(
            io::ErrorKind::InvalidInput,
            "PID must be positive and fit i32",
          )
        })?;
    let process = Process::new(pid).map_err(io::Error::other)?;
    let membership = process
      .cgroups()
      .map_err(io::Error::other)?
      .0
      .into_iter()
      .find(|entry| entry.hierarchy == 0 && entry.controllers.is_empty())
      .ok_or(Error::NotCgroupV2)?;
    let path = Self::membership_path(&membership.pathname)?;
    let mounts = Process::myself()
      .map_err(io::Error::other)?
      .mountinfo()
      .map_err(io::Error::other)?;
    let mut candidates = mounts
      .0
      .into_iter()
      .filter(|mount| mount.fs_type == "cgroup2")
      .filter_map(|mount| {
        path.strip_prefix(&mount.root).ok().map(|relative| {
          (
            Path::new(&mount.root).components().count(),
            mount.mount_point.join(relative),
          )
        })
      })
      .collect::<Vec<_>>();
    candidates.sort_by_key(|(depth, _)| std::cmp::Reverse(*depth));
    let mut last_error = Error::NotCgroupV2;
    for (_, candidate) in candidates {
      match Self::from_path(&candidate) {
        | Ok(cgroup) => return Ok(cgroup),
        | Err(error) => last_error = error,
      }
    }
    Err(last_error)
  }

  fn membership_path(path: &str) -> Result<&Path, Error> {
    match path.strip_suffix(" (deleted)") {
      | Some(path) => Err(Error::DeletedCgroup { path: path.into() }),
      | None => Ok(Path::new(path)),
    }
  }

  /// Opens a directory and verifies cgroup2 filesystem magic.
  /// Symlinks and parent traversal are rejected, including on old kernels.
  pub fn from_path(path: &Path) -> Result<Self, Error> {
    let (base, relative) = match path.is_absolute() {
      | true => (
        Path::new("/"),
        path.strip_prefix("/").map_err(io::Error::other)?,
      ),
      | false => (Path::new("."), path),
    };
    let base = fs::open(base, DIRECTORY_FLAGS, Mode::empty())
      .map_err(io::Error::from)?;
    let directory = Self::open_directory(&base, relative)?;
    Self::verified(directory, path.to_owned())
  }

  fn verified(directory: OwnedFd, path: PathBuf) -> Result<Self, Error> {
    match fs::fstatfs(&directory).map_err(io::Error::from)?.f_type {
      | CGROUP2_SUPER_MAGIC => Ok(Self { directory, path }),
      | _ => Err(Error::NotCgroupV2),
    }
  }

  fn open_directory(
    base: impl AsFd,
    relative: &Path,
  ) -> Result<OwnedFd, Error> {
    if relative
      .components()
      .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
      return Err(
        io::Error::new(
          io::ErrorKind::InvalidInput,
          "cgroup path must remain beneath its directory",
        )
        .into(),
      );
    }
    let relative = match relative.as_os_str().is_empty() {
      | true => Path::new("."),
      | false => relative,
    };
    match fs::openat2(
      &base,
      relative,
      DIRECTORY_FLAGS,
      Mode::empty(),
      ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS,
    ) {
      | Ok(fd) => Ok(fd),
      | Err(rustix::io::Errno::NOSYS) => Self::open_components(base, relative),
      | Err(error) => Err(io::Error::from(error).into()),
    }
  }

  fn open_components(
    base: impl AsFd,
    relative: &Path,
  ) -> Result<OwnedFd, Error> {
    let start = fs::openat(base, ".", DIRECTORY_FLAGS, Mode::empty())
      .map_err(io::Error::from)?;
    relative.components().try_fold(start, |directory, part| {
      fs::openat(&directory, part.as_os_str(), DIRECTORY_FLAGS, Mode::empty())
        .map_err(|error| Error::Io(error.into()))
    })
  }

  /// Opens one child by name; names containing slashes or traversal are invalid.
  pub fn child(&self, name: &OsStr) -> Result<Self, Error> {
    if name.as_bytes().contains(&b'/')
      || !matches!(
        Path::new(name).components().next(),
        Some(Component::Normal(_))
      )
    {
      return Err(
        io::Error::new(
          io::ErrorKind::InvalidInput,
          "child must be a single name",
        )
        .into(),
      );
    }
    Self::verified(
      Self::open_directory(&self.directory, Path::new(name))?,
      self.path.join(name),
    )
  }

  /// Lists directories, including child names that resemble interface files.
  /// A child removed during enumeration is omitted.
  pub fn children(&self) -> Result<Vec<Self>, Error> {
    Dir::read_from(&self.directory)
      .map_err(io::Error::from)?
      .filter_map(|entry| match entry {
        | Err(error) => Some(Err(Error::Io(error.into()))),
        | Ok(entry)
          if entry.file_type() == FileType::Directory
            && !matches!(entry.file_name().to_bytes(), b"." | b"..") =>
        {
          match self.child(OsStr::from_bytes(entry.file_name().to_bytes())) {
            | Err(Error::Io(error))
              if error.kind() == io::ErrorKind::NotFound =>
            {
              None
            },
            | result => Some(result),
          }
        },
        | Ok(_) => None,
      })
      .collect()
  }

  /// The path used to open the handle; it is diagnostic and may become stale.
  pub fn path(&self) -> &Path {
    &self.path
  }

  pub(crate) fn read(&self, file: &'static str) -> Result<String, Error> {
    super::io::read_file(&self.directory, file)
      .map_err(|error| Error::read(self.path.join(file), error))
  }

  pub(crate) fn parse<T: FromStr>(
    &self,
    file: &'static str,
  ) -> Result<T, Error>
  where
    T::Err: std::fmt::Display, {
    self
      .read(file)?
      .parse()
      .map_err(|error: T::Err| Error::Parse {
        file,
        detail: error.to_string(),
      })
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;
  use crate::v2::cpu::ReadCpu;

  #[test]
  fn missing_interfaces_keep_their_path() {
    let path =
      PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src/cgroup/v2"));
    // Internal fixture only: public constructors always verify filesystem magic.
    let cgroup = Cgroup {
      directory: assert_ok!(fs::open(&path, DIRECTORY_FLAGS, Mode::empty())),
      path: path.clone(),
    };
    assert!(
      matches!(cgroup.stat_local(), Err(Error::FileMissing { path: missing }) if missing == path.join("cpu.stat.local"))
    );
    for name in ["", ".", "..", "../cpu", "/cpu", "cpu/stat"] {
      assert!(
        matches!(cgroup.child(OsStr::new(name)), Err(Error::Io(error)) if error.kind() == io::ErrorKind::InvalidInput)
      );
    }
    for pid in [0, u32::MAX] {
      assert!(
        matches!(Cgroup::from_pid(pid), Err(Error::Io(error)) if error.kind() == io::ErrorKind::InvalidInput)
      );
    }
  }

  #[test]
  fn rejects_traversal_and_symlinks_with_both_backends() {
    let root = assert_ok!(fs::open("/", DIRECTORY_FLAGS, Mode::empty()));
    // /proc/self is a kernel-provided symlink, requiring no mutable fixture.
    assert_err!(Cgroup::open_directory(&root, Path::new("proc/self")));
    assert_err!(Cgroup::open_components(&root, Path::new("proc/self")));
    assert_err!(Cgroup::open_directory(&root, Path::new("../")));
    let proc = assert_ok!(Cgroup::open_components(&root, Path::new("proc")));
    assert_eq!(assert_ok!(fs::fstatfs(proc)).f_type, fs::PROC_SUPER_MAGIC);
  }

  #[test]
  fn recognizes_namespace_root_and_deleted_membership() {
    for path in ["/", "/user.slice/app.scope", "/space in name"] {
      assert_eq!(assert_ok!(Cgroup::membership_path(path)), Path::new(path));
    }
    assert!(
      matches!(Cgroup::membership_path("/gone (deleted)"), Err(Error::DeletedCgroup { path }) if path == Path::new("/gone"))
    );
  }
}
