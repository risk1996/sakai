use std::{
  fs::File,
  io::{self, Read},
  path::{Component, Path},
};

use rustix::{
  fd::AsFd,
  fs::{Mode, OFlags, openat},
};

/// Reads a cgroup v2 interface file relative to an open cgroup directory.
pub(crate) fn read_file(
  cgroup: impl AsFd,
  interface: &str,
) -> io::Result<String> {
  let mut components = Path::new(interface).components();

  match (components.next(), components.next()) {
    | (Some(Component::Normal(_)), None) => {},
    | _ => {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "cgroup interface must be a single relative file name",
      ));
    },
  }

  let descriptor = openat(
    cgroup,
    interface,
    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
    Mode::empty(),
  )
  .map_err(io::Error::from)?;
  let mut contents = String::new();
  File::from(descriptor).read_to_string(&mut contents)?;
  Ok(contents)
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;

  #[test]
  fn reads_relative_to_cgroup_directory() {
    let directory = assert_ok!(rustix::fs::open(
      concat!(env!("CARGO_MANIFEST_DIR"), "/src/cgroup/v2"),
      OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY,
      Mode::empty(),
    ));

    assert_eq!(
      assert_ok!(read_file(directory, "mod.rs")),
      include_str!("mod.rs")
    );
  }

  #[test]
  fn rejects_paths_outside_cgroup_directory() {
    let directory = assert_ok!(rustix::fs::open(
      concat!(env!("CARGO_MANIFEST_DIR"), "/src/cgroup/v2"),
      OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY,
      Mode::empty(),
    ));

    for interface in
      ["", ".", "..", "../common/mod.rs", "/etc/passwd", "cpu/stat"]
    {
      let error = assert_err!(read_file(&directory, interface));

      assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
  }

  #[test]
  fn does_not_follow_interface_symlinks() {
    let proc = assert_ok!(rustix::fs::open(
      "/proc",
      OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY,
      Mode::empty(),
    ));

    assert_err!(read_file(proc, "self"));
  }
}
