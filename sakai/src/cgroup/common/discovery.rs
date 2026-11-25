use std::io;

use procfs::process::Process;

use crate::Error;

pub(in crate::cgroup) fn processes(pid: u32) -> Result<(Process, Process), Error> {
  let pid = i32::try_from(pid)
    .ok()
    .filter(|pid| *pid > 0)
    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "PID must be positive and fit i32"))?;
  Ok((Process::new(pid).map_err(Error::procfs)?, Process::myself().map_err(Error::procfs)?))
}

// Try the candidates in order and retain the most specific mount's error.
pub(in crate::cgroup) fn open_candidates<C, T>(
  candidates: impl IntoIterator<Item = C>,
  mut open: impl FnMut(C) -> Result<T, Error>,
  missing_mount: impl FnOnce() -> Error,
) -> Result<T, Error> {
  let mut first_error = None;
  for candidate in candidates {
    match open(candidate) {
      | Ok(value) => return Ok(value),
      | Err(error) => {
        first_error.get_or_insert(error);
      },
    }
  }
  Err(first_error.unwrap_or_else(missing_mount))
}
