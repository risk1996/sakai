//! Task freezing state. The hierarchy root has no freezer files.
//!
//! See the [Linux v1 freezer guide](https://docs.kernel.org/admin-guide/cgroup-v1/freezer-subsystem.html).

use crate::{
  error::{ParseError, ParseValueError},
  parse::ParseBoolean,
};

/// Effective freezer state including inherited requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString)]
#[strum(serialize_all = "UPPERCASE")]
#[non_exhaustive]
pub enum FreezerState {
  /// No freeze is in progress.
  Thawed,
  /// Tasks are transitioning to the frozen state.
  Freezing,
  /// All tasks in the cgroup and its descendants are frozen.
  Frozen,
}

impl std::str::FromStr for FreezerSnapshot {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let state = contents.trim();
    state.parse().map(Self).map_err(|error| ParseError::invalid(contents, "state", state, ParseValueError::from(error)))
  }
}

/// Observed freezer state from the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct FreezerSnapshot(
  /// Effective state of the cgroup and its descendants.
  pub FreezerState,
);

/// Requests a change to this cgroup's own freezer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
#[strum(serialize_all = "UPPERCASE")]
#[non_exhaustive]
pub enum FreezerRequest {
  /// Clear this group's freeze request. An ancestor can still keep it frozen.
  Thawed,
  /// Request freezing the cgroup and all of its descendants.
  Frozen,
}

v1_file!(FreezerSnapshot, "freezer.state", Read);
v1_file!(write FreezerSnapshot, 'a => FreezerRequest, |request: FreezerRequest| Ok(request.to_string()));

v1_scalar!(
  /// Whether this cgroup has requested freezing itself.
  SelfFreezing,
  "freezer.self_freezing",
  bool,
  ParseBoolean,
  Read
);
v1_scalar!(
  /// Whether freezing was requested by an ancestor.
  ParentFreezing,
  "freezer.parent_freezing",
  bool,
  ParseBoolean,
  Read
);
