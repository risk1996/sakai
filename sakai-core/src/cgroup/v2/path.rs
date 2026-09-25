use std::path::{Path, PathBuf};

use procfs::process::Process;

/// The directory containing a process's cgroup v2 interface files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgroupPath {
  path: PathBuf,
  mount_point: PathBuf,
  read_only: bool,
}

impl CgroupPath {
  /// Discovers the current process's cgroup v2 directory from procfs.
  pub fn current() -> Result<Self, CgroupPathError> {
    Self::of_process(&Process::myself()?)
  }

  fn of_process(process: &Process) -> Result<Self, CgroupPathError> {
    let cgroup = process
      .cgroups()?
      .0
      .into_iter()
      .find(|cgroup| cgroup.hierarchy == 0 && cgroup.controllers.is_empty())
      .ok_or(CgroupPathError::UnifiedHierarchyNotFound)?;
    let cgroup = Path::new(&cgroup.pathname);
    let (mount, relative) = process
      .mountinfo()?
      .0
      .into_iter()
      .filter(|mount| mount.fs_type == "cgroup2")
      .filter_map(|mount| {
        cgroup
          .strip_prefix(Path::new(&mount.root))
          .ok()
          .map(|relative| (mount, relative))
      })
      .max_by_key(|(mount, _)| Path::new(&mount.root).components().count())
      .ok_or(CgroupPathError::UnifiedMountNotFound)?;

    Ok(Self {
      path: mount.mount_point.join(relative),
      mount_point: mount.mount_point,
      read_only: mount.mount_options.contains_key("ro"),
    })
  }

  /// Returns the cgroup2 mount point that exposes this cgroup.
  pub fn mount_point(&self) -> &Path {
    &self.mount_point
  }

  /// Returns whether the process sees the cgroup2 mount as read-only.
  pub const fn is_read_only(&self) -> bool {
    self.read_only
  }
}

impl AsRef<Path> for CgroupPath {
  fn as_ref(&self) -> &Path {
    &self.path
  }
}

/// An error discovering a process's cgroup v2 directory.
#[derive(Debug, thiserror::Error)]
pub enum CgroupPathError {
  /// Procfs could not be read or parsed.
  #[error(transparent)]
  Procfs(#[from] procfs::ProcError),
  /// The process has no entry for the unified hierarchy.
  #[error("the process does not belong to a cgroup v2 hierarchy")]
  UnifiedHierarchyNotFound,
  /// No cgroup2 mount exposes the process's hierarchy path.
  #[error("no cgroup2 mount exposes the process's cgroup")]
  UnifiedMountNotFound,
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;

  use super::*;

  #[test]
  fn discovers_cgroup_paths_from_procfs() {
    for (fixture, expected) in [
      ("unified", CgroupPath {
        path: PathBuf::from("/sys/fs/cgroup/user.slice/app.scope"),
        mount_point: PathBuf::from("/sys/fs/cgroup"),
        read_only: true,
      }),
      ("bind_mount", CgroupPath {
        path: PathBuf::from("/run/delegated/process.scope"),
        mount_point: PathBuf::from("/run/delegated"),
        read_only: false,
      }),
    ] {
      let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/cgroup/v2/fixtures")
        .join(fixture)
        .join("1");
      let process = assert_ok!(Process::new_with_root(root));

      assert_eq!(
        assert_ok!(CgroupPath::of_process(&process)),
        expected,
        "fixture: {fixture}"
      );
    }
  }
}
