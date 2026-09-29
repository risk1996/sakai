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
    Self::candidates(process, process)?
      .into_iter()
      .next()
      .ok_or(CgroupPathError::UnifiedMountNotFound)
  }

  /// Resolves a target membership against mounts visible to the caller.
  pub(super) fn candidates(
    target: &Process,
    caller: &Process,
  ) -> Result<Vec<Self>, CgroupPathError> {
    let membership = target
      .cgroups()?
      .0
      .into_iter()
      .find(|entry| entry.hierarchy == 0 && entry.controllers.is_empty())
      .ok_or(CgroupPathError::UnifiedHierarchyNotFound)?;
    let cgroup = match membership.pathname.strip_suffix(" (deleted)") {
      | Some(path) => {
        return Err(CgroupPathError::DeletedCgroup { path: path.into() });
      },
      | None => Path::new(&membership.pathname),
    };
    let mut candidates = caller
      .mountinfo()?
      .0
      .into_iter()
      .filter(|mount| mount.fs_type == "cgroup2")
      .filter_map(|mount| {
        let root = Path::new(&mount.root);
        let relative = cgroup.strip_prefix(root).ok()?;
        Some((root.components().count(), Self {
          path: mount.mount_point.join(relative),
          mount_point: mount.mount_point,
          read_only: mount.mount_options.contains_key("ro"),
        }))
      })
      .collect::<Vec<_>>();
    candidates.sort_by_key(|(depth, _)| std::cmp::Reverse(*depth));
    match candidates.is_empty() {
      | true => Err(CgroupPathError::UnifiedMountNotFound),
      | false => Ok(candidates.into_iter().map(|(_, path)| path).collect()),
    }
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
  /// Procfs reports a cgroup that has already been removed.
  #[error("cgroup has been deleted: {path}")]
  DeletedCgroup { path: PathBuf },
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
      ("namespace_root", CgroupPath {
        path: PathBuf::from("/sys/fs/cgroup"),
        mount_point: PathBuf::from("/sys/fs/cgroup"),
        read_only: true,
      }),
      ("multiple_mounts", CgroupPath {
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("src/cgroup/v2/fixtures/deleted/1");
    let process = assert_ok!(Process::new_with_root(root));
    assert!(matches!(
      CgroupPath::of_process(&process),
      Err(CgroupPathError::DeletedCgroup { path }) if path == Path::new("/gone")
    ));
  }

  #[test]
  fn orders_matching_mounts_for_fallback() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("src/cgroup/v2/fixtures/multiple_mounts/1");
    let process = assert_ok!(Process::new_with_root(root));
    let candidates = assert_ok!(CgroupPath::candidates(&process, &process));
    assert_eq!(candidates, [
      CgroupPath {
        path: "/run/delegated/process.scope".into(),
        mount_point: "/run/delegated".into(),
        read_only: false,
      },
      CgroupPath {
        path: "/run/parent/pod-1/process.scope".into(),
        mount_point: "/run/parent".into(),
        read_only: true,
      },
      CgroupPath {
        path: "/sys/fs/cgroup/kubepods.slice/pod-1/process.scope".into(),
        mount_point: "/sys/fs/cgroup".into(),
        read_only: true,
      },
    ]);
  }

  #[test]
  fn resolves_target_membership_in_callers_mount_namespace() {
    let fixtures =
      Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cgroup/v2/fixtures");
    let target =
      assert_ok!(Process::new_with_root(fixtures.join("bind_mount/1")));
    let caller = assert_ok!(Process::new_with_root(fixtures.join("unified/1")));
    let candidates = assert_ok!(CgroupPath::candidates(&target, &caller));
    assert_eq!(
      candidates[0].as_ref(),
      Path::new("/sys/fs/cgroup/kubepods.slice/pod-1/process.scope")
    );
  }
}
