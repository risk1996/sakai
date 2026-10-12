use std::str::FromStr;

use crate::error::{ParseError, ParseValueError};

/// A snapshot of the request-priority policy in `io.prio.class`.
///
/// Requires `CONFIG_BLK_CGROUP_IOPRIO`. This controls I/O request priority,
/// including supported page-cache writeback, rather than CPU priority.
/// See the [Linux I/O priority controller source](https://github.com/torvalds/linux/blob/v6.5/block/blk-ioprio.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
#[strum(serialize_all = "kebab-case")]
#[non_exhaustive]
pub enum IoPriorityClass {
  /// Keep the request's priority class unchanged.
  NoChange,
  /// Promote non-realtime requests to realtime class, priority level 4.
  ///
  /// Since Linux 6.5; see the [Linux 6.5 source](https://github.com/torvalds/linux/blob/v6.5/block/blk-ioprio.c).
  PromoteToRt,
  /// Restrict unclassified or realtime requests to best-effort class.
  RestrictToBe,
  /// Assign idle class to every request.
  Idle,
  /// Legacy `none-to-rt` policy; an alias of `promote-to-rt` since Linux 6.5.
  ///
  /// Earlier kernels only promote requests without an explicit class.
  /// See the [Linux 6.4 source](https://github.com/torvalds/linux/blob/v6.4/block/blk-ioprio.c)
  /// and [Linux 6.5 source](https://github.com/torvalds/linux/blob/v6.5/block/blk-ioprio.c).
  NoneToRt,
}

impl IoPriorityClass {
  /// The I/O priority policy interface filename, also present at the root.
  pub const FILE_NAME: &'static str = "io.prio.class";
}

// Parse the kernel's single newline-terminated value with contextual errors.
impl FromStr for IoPriorityClass {
  type Err = ParseError<ParseValueError>;

  fn from_str(raw: &str) -> Result<Self, Self::Err> {
    let value = raw.trim();
    if value.is_empty() {
      return Err(ParseError::missing(raw, "priority"));
    }
    match value {
      | "no-change" => Ok(Self::NoChange),
      | "promote-to-rt" => Ok(Self::PromoteToRt),
      | "restrict-to-be" => Ok(Self::RestrictToBe),
      | "idle" => Ok(Self::Idle),
      | "none-to-rt" => Ok(Self::NoneToRt),
      | _ => Err(ParseError::invalid(raw, "priority", value, ParseValueError::OutOfRange)),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Invalid, Missing},
  };

  #[test]
  fn parses_current_and_legacy_priority_policies() {
    Cases::<IoPriorityClass>::check([
      ("no-change\n", Ok(IoPriorityClass::NoChange)),
      ("promote-to-rt\n", Ok(IoPriorityClass::PromoteToRt)),
      ("restrict-to-be", Ok(IoPriorityClass::RestrictToBe)),
      ("idle", Ok(IoPriorityClass::Idle)),
      ("none-to-rt", Ok(IoPriorityClass::NoneToRt)),
      ("", Err(Missing("priority"))),
      ("future", Err(Invalid("priority", "future"))),
    ]);
  }
}
