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

use super::{
  core::Core,
  cpu::Cpu,
  memory::Memory,
  path::{CgroupPath, CgroupPathError},
};
use crate::error::Error;

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
    let caller = Process::myself().map_err(io::Error::other)?;
    let candidates = CgroupPath::candidates(&process, &caller).map_err(
      |error| match error {
        | CgroupPathError::Procfs(source) => {
          Error::Io(io::Error::other(source))
        },
        | CgroupPathError::DeletedCgroup { path } => {
          Error::DeletedCgroup { path }
        },
        | CgroupPathError::UnifiedHierarchyNotFound
        | CgroupPathError::UnifiedMountNotFound => Error::NotCgroupV2,
      },
    )?;
    Self::open_candidates(candidates, Self::from_path)
  }

  fn open_candidates<T>(
    candidates: impl IntoIterator<Item = CgroupPath>,
    mut open: impl FnMut(&Path) -> Result<T, Error>,
  ) -> Result<T, Error> {
    let mut last_error = Error::NotCgroupV2;
    for candidate in candidates {
      match open(candidate.as_ref()) {
        | Ok(value) => return Ok(value),
        | Err(error) => last_error = error,
      }
    }
    Err(last_error)
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

  /// Borrows this handle to read CPU controller interfaces.
  pub fn cpu(&self) -> Cpu<'_> {
    Cpu { cgroup: self }
  }

  /// Borrows this handle to read memory controller interfaces.
  pub fn memory(&self) -> Memory<'_> {
    Memory { cgroup: self }
  }

  /// Borrows this handle to read core cgroup interfaces.
  pub fn core(&self) -> Core<'_> {
    Core { cgroup: self }
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
    T::Err: std::error::Error + Send + Sync + 'static, {
    self
      .read(file)?
      .parse()
      .map_err(|error: T::Err| Error::Parse {
        path: self.path.join(file),
        source: Box::new(error),
      })
  }
}

#[cfg(test)]
mod tests {
  use std::error::Error as _;

  use assertables::{assert_err, assert_ok};

  use super::*;
  use crate::{
    error::{ParseError, ParseValueError},
    v2::cpu::CpuIdle,
  };

  #[test]
  fn parse_failure_keeps_path_and_typed_source() {
    let path =
      PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src/cgroup/v2"));
    let cgroup = Cgroup {
      directory: assert_ok!(fs::open(&path, DIRECTORY_FLAGS, Mode::empty())),
      path: path.clone(),
    };
    let error = assert_err!(cgroup.parse::<CpuIdle>("mod.rs"));
    let Error::Parse {
      path: actual,
      source,
    } = &error
    else {
      panic!("expected parse failure, got {error:?}");
    };
    assert_eq!(actual, &path.join("mod.rs"));
    let parsed = source
      .downcast_ref::<ParseError<ParseValueError>>()
      .expect("original parse error");
    assert_eq!(
      error.source().map(ToString::to_string),
      Some(parsed.to_string())
    );
    assert!(matches!(parsed, ParseError::Invalid { field: "idle", .. }));
    assert!(parsed.source().is_some());
    assert!(error.to_string().contains("failed to parse"));
    assert!(error.to_string().contains("cgroup content"));
  }

  #[test]
  fn cgroup_type_failure_keeps_raw_input_and_strum_source() {
    let path = PathBuf::from(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/src/cgroup/v2/fixtures/invalid_type"
    ));
    let cgroup = Cgroup {
      directory: assert_ok!(fs::open(&path, DIRECTORY_FLAGS, Mode::empty())),
      path: path.clone(),
    };
    let error = assert_err!(cgroup.core().kind());
    let Error::Parse { path: actual, .. } = &error else {
      panic!("expected parse failure, got {error:?}");
    };
    assert_eq!(actual, &path.join("cgroup.type"));
    assert!(error.to_string().contains("\"unknown\\n\""));
    let reason = error
      .source()
      .and_then(std::error::Error::source)
      .expect("strum parse reason");
    assert!(error.to_string().contains(&reason.to_string()));
  }

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
      matches!(cgroup.cpu().stat_local(), Err(Error::FileMissing { path: missing }) if missing == path.join("cpu.stat.local"))
    );
    assert!(
      matches!(cgroup.core().kind(), Err(Error::FileMissing { path: missing }) if missing == path.join("cgroup.type"))
    );
    assert!(
      matches!(cgroup.memory().current(), Err(Error::FileMissing { path: missing }) if missing == path.join("memory.current"))
    );
    assert!(
      matches!(cgroup.memory().max(), Err(Error::FileMissing { path: missing }) if missing == path.join("memory.max"))
    );
    assert!(
      matches!(cgroup.memory().high(), Err(Error::FileMissing { path: missing }) if missing == path.join("memory.high"))
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
  fn tries_next_matching_mount_after_open_failure() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("src/cgroup/v2/fixtures/multiple_mounts/1");
    let process = assert_ok!(Process::new_with_root(root));
    let candidates = assert_ok!(CgroupPath::candidates(&process, &process));
    let mut attempted = Vec::new();
    let selected = assert_ok!(Cgroup::open_candidates(candidates, |path| {
      attempted.push(path.to_owned());
      match attempted.len() {
        | 1 => Err(Error::NotCgroupV2),
        | _ => Ok(path.to_owned()),
      }
    }));
    assert_eq!(selected, Path::new("/run/parent/pod-1/process.scope"));
    assert_eq!(attempted, [
      PathBuf::from("/run/delegated/process.scope"),
      PathBuf::from("/run/parent/pod-1/process.scope"),
    ]);
  }
}
