use std::{fs, io, path::Path};

/// Reads a cgroup interface file as UTF-8 text.
pub fn read_file(path: impl AsRef<Path>) -> io::Result<String> {
  fs::read_to_string(path)
}
