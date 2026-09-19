use std::{
  io,
  path::{Component, Path},
};

use rustix::{
  fd::AsFd,
  fs::{Mode, OFlags, openat},
};

const READ_BUFFER_SIZE: usize = 8 * 1024;

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
  .map_err(io_error)?;
  let mut contents = Vec::new();

  loop {
    let mut buffer = [0_u8; READ_BUFFER_SIZE];
    match rustix::io::read(&descriptor, &mut buffer) {
      | Ok(0) => break,
      | Ok(count) => contents.extend(buffer.iter().take(count).copied()),
      | Err(rustix::io::Errno::INTR) => {},
      | Err(error) => return Err(io_error(error)),
    }
  }

  String::from_utf8(contents)
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn io_error(error: rustix::io::Errno) -> io::Error {
  io::Error::from_raw_os_error(error.raw_os_error())
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

    for interface in ["", ".", "..", "../common/mod.rs", "/etc/passwd"] {
      let error = assert_err!(read_file(&directory, interface));

      assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
  }
}
