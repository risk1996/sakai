use nutype::nutype;

#[cfg(any(target_os = "linux", test))]
use crate::error::{ParseError, ParseValueError};

/// A process or thread ID reported in the reader's PID namespace.
///
/// `cgroup.procs` reports process leaders; `cgroup.threads` reports individual
/// threads. These IDs can be recycled immediately after a read and do not pin
/// processes. Zero represents a task invisible in that namespace, not a
/// process to target. `try_new` validates the nonnegative signed 32-bit range;
/// `AsRef` and `into_inner` expose the numeric identifier.
/// See [Linux process and thread lists](https://docs.kernel.org/admin-guide/cgroup-v2.html#core-interface-files).
/// For namespace-invisible zero entries, see [kernel list output](https://github.com/torvalds/linux/blob/v6.1/kernel/cgroup/cgroup.c)
/// and [PID namespace conversion](https://github.com/torvalds/linux/blob/v6.1/include/linux/pid.h).
#[nutype(validate(predicate = |id| *id >= 0), derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRef))]
pub struct TaskId(i32);

impl TaskId {
  /// The process list interface filename.
  pub const PROCS_FILE_NAME: &'static str = "cgroup.procs";
  /// The thread list interface filename.
  pub const THREADS_FILE_NAME: &'static str = "cgroup.threads";

  #[cfg(any(target_os = "linux", test))]
  pub(super) fn parse_list(contents: &str) -> Result<Vec<Self>, ParseError<ParseValueError>> {
    contents
      .lines()
      .filter(|line| !line.trim().is_empty())
      .map(|line| {
        let raw = line.trim();
        let id = raw.parse::<i32>().map_err(|error| ParseError::invalid(contents, "task_id", raw, error.into()))?;
        Self::try_new(id).map_err(|_error| ParseError::invalid(contents, "task_id", raw, ParseValueError::OutOfRange))
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;

  #[test]
  fn preserves_task_list_order_duplicates_and_empty_membership() {
    let expected = [42, 0, 7, 42, i32::MAX].map(|id| assert_ok!(TaskId::try_new(id))).to_vec();
    assert_eq!(assert_ok!(TaskId::parse_list("42\n0\n7\n42\n2147483647\n")), expected);
    assert_eq!(assert_ok!(TaskId::parse_list("")), vec![]);
    for input in ["-1", "2147483648", "nope", "1 2"] {
      assert_err!(TaskId::parse_list(input));
    }
  }
}
