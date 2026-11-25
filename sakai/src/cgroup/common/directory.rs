use std::{
  ffi::OsStr,
  io,
  os::unix::ffi::OsStrExt,
  path::{Component, Path, PathBuf},
};

use rustix::{
  fd::{AsFd, OwnedFd},
  fs::{self, Dir, FileType, Mode, OFlags, ResolveFlags},
};

use super::{ReadableFile, WritableFile, file};
use crate::Error;

const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY.union(OFlags::CLOEXEC).union(OFlags::NOFOLLOW).union(OFlags::DIRECTORY);

#[derive(Debug)]
pub(in crate::cgroup) struct Directory {
  pub(in crate::cgroup) fd: OwnedFd,
  pub(in crate::cgroup) path: PathBuf,
}

impl Directory {
  pub(in crate::cgroup) fn read<F: ReadableFile>(&self, name: &str) -> Result<F::Value, Error> {
    let path = self.path.join(name);
    let raw = file::read_file(&self.fd, name).map_err(|source| Error::read(path.clone(), source))?;
    F::decode(&raw).map_err(|source| Error::Parse { path, source })
  }

  pub(in crate::cgroup) fn write<F: WritableFile>(&self, name: &str, command: F::Command<'_>) -> Result<(), Error> {
    let path = self.path.join(name);
    let command = F::encode(command).map_err(|source| Error::Encode { path: path.clone(), source })?;
    file::write_file(&self.fd, name, &command)
      .map_err(|source| Error::interface_operation("write interface", path, source))
  }

  pub(in crate::cgroup) fn device(&self) -> Result<fs::Dev, Error> {
    fs::fstat(&self.fd).map(|stat| stat.st_dev).map_err(|source| Error::Operation {
      operation: "stat cgroup",
      path: self.path.clone(),
      source: source.into(),
    })
  }

  pub(in crate::cgroup) fn verify(&self, magic: fs::FsWord, wrong_fs: Error) -> Result<(), Error> {
    let stat = fs::fstatfs(&self.fd).map_err(|source| Error::Operation {
      operation: "stat cgroup filesystem",
      path: self.path.clone(),
      source: source.into(),
    })?;
    if stat.f_type == magic { Ok(()) } else { Err(wrong_fs) }
  }

  pub(in crate::cgroup) fn open(path: &Path) -> Result<Self, Error> {
    Self::open_path(path).map_err(|error| match error {
      | Error::Io(source) if source.kind() != io::ErrorKind::InvalidInput => {
        Error::Operation { operation: "open cgroup", path: path.to_owned(), source }
      },
      | other => other,
    })
  }

  fn open_path(path: &Path) -> Result<Self, Error> {
    let (base, relative) = match path.is_absolute() {
      | true => (Path::new("/"), path.strip_prefix("/").map_err(io::Error::other)?),
      | false => (Path::new("."), path),
    };
    let base = fs::open(base, DIRECTORY_FLAGS, Mode::empty()).map_err(io::Error::from)?;
    let directory = Self::open_directory(&base, relative)?;
    Ok(Self { fd: directory, path: path.to_owned() })
  }

  pub(in crate::cgroup) fn open_directory(base: impl AsFd, relative: &Path) -> Result<OwnedFd, Error> {
    if relative.components().any(|part| !matches!(part, Component::Normal(_) | Component::CurDir)) {
      return Err(io::Error::new(io::ErrorKind::InvalidInput, "cgroup path must remain beneath its directory").into());
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

  pub(in crate::cgroup) fn open_components(base: impl AsFd, relative: &Path) -> Result<OwnedFd, Error> {
    let start = fs::openat(base, ".", DIRECTORY_FLAGS, Mode::empty()).map_err(io::Error::from)?;
    relative.components().try_fold(start, |directory, part| {
      fs::openat(&directory, part.as_os_str(), DIRECTORY_FLAGS, Mode::empty()).map_err(|error| Error::Io(error.into()))
    })
  }

  pub(in crate::cgroup) fn child(&self, name: &OsStr) -> Result<Self, Error> {
    if name.as_bytes().contains(&b'/') || !matches!(Path::new(name).components().next(), Some(Component::Normal(_))) {
      return Err(io::Error::new(io::ErrorKind::InvalidInput, "child must be a single name").into());
    }
    let path = self.path.join(name);
    let directory = Self::open_directory(&self.fd, Path::new(name)).map_err(|error| match error {
      | Error::Io(source) => Error::Operation { operation: "open child cgroup", path: path.clone(), source },
      | other => other,
    })?;
    Ok(Self { fd: directory, path })
  }

  pub(in crate::cgroup) fn children<T>(
    &self,
    mut open: impl FnMut(&OsStr) -> Result<T, Error>,
  ) -> Result<Vec<T>, Error> {
    Dir::read_from(&self.fd)
      .map_err(|source| Error::Operation {
        operation: "list child cgroups",
        path: self.path.clone(),
        source: source.into(),
      })?
      .filter_map(|entry| match entry {
        | Err(source) => Some(Err(Error::Operation {
          operation: "list child cgroups",
          path: self.path.clone(),
          source: source.into(),
        })),
        | Ok(entry)
          if entry.file_type() == FileType::Directory && !matches!(entry.file_name().to_bytes(), b"." | b"..") =>
        {
          match open(OsStr::from_bytes(entry.file_name().to_bytes())) {
            | Err(Error::Operation { source, .. }) if source.kind() == io::ErrorKind::NotFound => None,
            | result => Some(result),
          }
        },
        | Ok(_) => None,
      })
      .collect()
  }
}
