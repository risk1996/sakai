use std::{
  fs::File,
  io::{self, Read},
  path::{Component, Path},
};

use rustix::{
  fd::AsFd,
  fs::{Mode, OFlags, openat},
};

/// Reads a cgroup interface file relative to an open cgroup directory.
pub(crate) fn read_file(cgroup: impl AsFd, interface: &str) -> io::Result<String> {
  let descriptor = open_interface(cgroup, interface, OFlags::RDONLY)?;
  let mut contents = String::new();
  File::from(descriptor).read_to_string(&mut contents)?;
  Ok(contents)
}

fn open_interface(cgroup: impl AsFd, interface: &str, access: OFlags) -> io::Result<rustix::fd::OwnedFd> {
  let mut components = Path::new(interface).components();
  match (components.next(), components.next()) {
    | (Some(Component::Normal(name)), None) if name == interface => {},
    | _ => {
      return Err(io::Error::new(io::ErrorKind::InvalidInput, "cgroup interface must be a single relative file name"));
    },
  }
  openat(cgroup, interface, access | OFlags::CLOEXEC | OFlags::NOFOLLOW, Mode::empty()).map_err(io::Error::from)
}

pub(crate) fn write_file(cgroup: impl AsFd, interface: &str, command: &str) -> io::Result<()> {
  if command.is_empty() {
    return Err(io::Error::new(io::ErrorKind::InvalidInput, "cgroup command must not be empty"));
  }
  let descriptor = open_interface(cgroup, interface, OFlags::WRONLY)?;
  write_command(command.as_bytes(), |bytes| rustix::io::write(&descriptor, bytes))
}

fn write_command(command: &[u8], mut send: impl FnMut(&[u8]) -> rustix::io::Result<usize>) -> io::Result<()> {
  loop {
    match send(command) {
      | Ok(written) if written == command.len() => return Ok(()),
      | Ok(written) => {
        return Err(io::Error::new(
          io::ErrorKind::WriteZero,
          format!("wrote {written} of {} command bytes", command.len()),
        ));
      },
      | Err(rustix::io::Errno::INTR) => {},
      | Err(error) => return Err(error.into()),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{fs, os::unix::fs::symlink};

  use assertables::{assert_err, assert_ok};

  use super::*;

  #[test]
  fn reads_relative_to_cgroup_directory() {
    let directory = assert_ok!(rustix::fs::open(
      concat!(env!("CARGO_MANIFEST_DIR"), "/src/cgroup"),
      OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY,
      Mode::empty(),
    ));

    assert_eq!(assert_ok!(read_file(directory, "mod.rs")), include_str!("mod.rs"));
  }

  #[test]
  fn rejects_paths_outside_cgroup_directory() {
    let directory = assert_ok!(rustix::fs::open(
      concat!(env!("CARGO_MANIFEST_DIR"), "/src/cgroup/v2"),
      OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY,
      Mode::empty(),
    ));

    for interface in ["", ".", "..", "../parse/mod.rs", "/etc/passwd", "cpu/stat"] {
      let error = assert_err!(read_file(&directory, interface));

      assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
  }

  #[test]
  fn does_not_follow_interface_symlinks() {
    let proc =
      assert_ok!(rustix::fs::open("/proc", OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY, Mode::empty(),));

    assert_err!(read_file(proc, "self"));
  }

  #[test]
  fn writes_only_existing_interfaces_relative_to_directory() {
    let scratch = assert_ok!(tempfile::tempdir());
    let directory = assert_ok!(fs::File::open(scratch.path()));
    let path = scratch.path().join("cpu.max");
    assert_ok!(fs::write(&path, "max 100000\n"));
    assert_ok!(symlink("cpu.max", scratch.path().join("alias")));

    for name in ["./cpu.max", "cpu.max/", "../cpu.max", "/cpu.max"] {
      assert_eq!(assert_err!(write_file(&directory, name, "25000 50000")).kind(), io::ErrorKind::InvalidInput);
    }
    assert_eq!(assert_err!(write_file(&directory, "missing", "25000 50000")).kind(), io::ErrorKind::NotFound);
    assert!(!scratch.path().join("missing").exists());
    assert_eq!(
      assert_err!(write_file(&directory, "alias", "25000 50000")).raw_os_error(),
      Some(rustix::io::Errno::LOOP.raw_os_error()),
    );
    assert_ok!(write_file(&directory, "cpu.max", "25000 50000"));
    assert_eq!(assert_ok!(fs::read_to_string(path)), "25000 50000");
  }

  #[test]
  fn retries_interruptions_but_does_not_send_the_remainder_of_a_short_write() {
    let command = b"25000 50000";
    for partial in [0, 2] {
      let mut responses = [Err(rustix::io::Errno::INTR), Ok(partial)].into_iter();
      let error = assert_err!(write_command(command, |bytes| {
        assert_eq!(bytes, command);
        responses.next().expect("unexpected additional write")
      }));
      assert_eq!(error.kind(), io::ErrorKind::WriteZero);
      assert_eq!(responses.next(), None);
    }
  }
}
