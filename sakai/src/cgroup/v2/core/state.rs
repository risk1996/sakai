use std::{collections::HashMap, str::FromStr};

use super::CgroupController;
use crate::{
  error::{ParseError, ParseValueError},
  parse::{KeyedFields, ParseBoolean, ParseCgroup, ParseCount},
  unit::Count,
};

/// A point-in-time snapshot of `cgroup.events` lifecycle state.
///
/// Boolean states are encoded as zero or one. They may change immediately
/// after a read. Older kernels omit `frozen`; absence does not imply false.
/// Unknown future keys are ignored. The hierarchy root lacks this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CgroupEvents {
  populated: bool,
  frozen: Option<bool>,
}

impl CgroupEvents {
  /// The cgroup v2 `cgroup.events` interface filename.
  pub const FILE_NAME: &'static str = "cgroup.events";

  /// Whether this cgroup or any descendant contains live processes.
  #[must_use]
  pub const fn populated(self) -> bool { self.populated }

  /// Whether freezing has completed, if the kernel reports this state.
  ///
  /// Includes freezing imposed by ancestors; this is the observed state,
  /// which may lag a request written to `cgroup.freeze`.
  #[must_use]
  pub const fn frozen(self) -> Option<bool> { self.frozen }
}

impl FromStr for CgroupEvents {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(contents, |_| "value")?;
    Ok(Self {
      populated: fields.required::<ParseBoolean, _>("populated")?,
      frozen: fields.optional::<ParseBoolean, _>("frozen")?,
    })
  }
}

/// A point-in-time snapshot of `cgroup.stat` object counts.
///
/// All quantities are counts of cgroups or subsystem states, not processes
/// or bytes. They can increase or decrease between reads. Dying objects have
/// been removed but remain alive until kernel references are released.
/// Older kernels omit subsystem counters; absent entries do not imply zero.
/// Unknown keys are ignored, while subsystem names are retained even for
/// controllers not yet known to this library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgroupStat {
  descendants: Count,
  dying_descendants: Count,
  subsystems: HashMap<CgroupController, Count>,
  dying_subsystems: HashMap<CgroupController, Count>,
}

impl CgroupStat {
  /// The cgroup v2 `cgroup.stat` interface filename.
  pub const FILE_NAME: &'static str = "cgroup.stat";

  /// Visible descendant cgroups, excluding this cgroup.
  #[must_use]
  pub const fn descendants(&self) -> Count { self.descendants }

  /// Deleted descendant cgroups that have not yet been destroyed.
  #[must_use]
  pub const fn dying_descendants(&self) -> Count { self.dying_descendants }

  /// Live subsystem states at and beneath this cgroup, keyed by controller.
  #[must_use]
  pub const fn subsystems(&self) -> &HashMap<CgroupController, Count> { &self.subsystems }

  /// Dying subsystem states at and beneath this cgroup, keyed by controller.
  #[must_use]
  pub const fn dying_subsystems(&self) -> &HashMap<CgroupController, Count> { &self.dying_subsystems }

  fn parse_subsystems(
    raw: &str,
    fields: &KeyedFields<'_>,
    prefix: &'static str,
  ) -> Result<HashMap<CgroupController, Count>, ParseError<ParseValueError>> {
    fields
      .iter()
      .filter_map(|(key, value)| key.strip_prefix(prefix).map(|name| (name, value)))
      .map(|(name, value)| match name.is_empty() {
        | true => Err(ParseError::invalid(raw, "controller", name, ParseValueError::OutOfRange)),
        | false => Ok((CgroupController::from(name), Count::parse_field(raw, prefix, value)?)),
      })
      .collect()
  }
}

impl FromStr for CgroupStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(contents, |_| "value")?;
    Ok(Self {
      descendants: fields.required::<ParseCount, _>("nr_descendants")?,
      dying_descendants: fields.required::<ParseCount, _>("nr_dying_descendants")?,
      subsystems: Self::parse_subsystems(contents, &fields, "nr_subsys_")?,
      dying_subsystems: Self::parse_subsystems(contents, &fields, "nr_dying_subsys_")?,
    })
  }
}

#[cfg(test)]
mod tests {
  use indoc::indoc;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

  #[test]
  fn parses_lifecycle_states_and_older_kernel_absence() {
    Cases::<CgroupEvents>::check([
      (
        indoc! {"
        frozen 1
        future_state nope
        populated 0
      "},
        Ok(CgroupEvents { populated: false, frozen: Some(true) }),
      ),
      ("populated 1\nfrozen 0\n", Ok(CgroupEvents { populated: true, frozen: Some(false) })),
      ("populated 1\n", Ok(CgroupEvents { populated: true, frozen: None })),
      ("", Err(Missing("populated"))),
      ("frozen 0", Err(Missing("populated"))),
      ("populated", Err(Missing("value"))),
      ("populated 2", Err(Invalid("populated", "2"))),
      ("populated -1", Err(Invalid("populated", "-1"))),
      ("populated nope", Err(Invalid("populated", "nope"))),
      ("populated 1\nfrozen 2", Err(Invalid("frozen", "2"))),
      ("populated 1 extra", Err(Excess)),
    ]);
  }

  #[test]
  fn parses_descendants_and_optional_subsystem_counts() {
    let count = |value| Count { value, ..Default::default() };
    Cases::<CgroupStat>::check([
      (
        indoc! {"
        nr_dying_subsys_memory 2
        nr_subsys_future_controller 3
        nr_dying_descendants 1
        future_metric nope
        nr_subsys_memory 4
        nr_descendants 5
        nr_dying_subsys_cpu 0
      "},
        Ok(CgroupStat {
          descendants: count(5),
          dying_descendants: count(1),
          subsystems: HashMap::from([
            (CgroupController::Memory, count(4)),
            (CgroupController::Other("future_controller".into()), count(3)),
          ]),
          dying_subsystems: HashMap::from([(CgroupController::Memory, count(2)), (CgroupController::Cpu, count(0))]),
        }),
      ),
      (
        "nr_descendants 0\nnr_dying_descendants 18446744073709551615\n",
        Ok(CgroupStat {
          descendants: count(0),
          dying_descendants: count(u64::MAX),
          subsystems: HashMap::new(),
          dying_subsystems: HashMap::new(),
        }),
      ),
      ("", Err(Missing("nr_descendants"))),
      ("nr_descendants 0", Err(Missing("nr_dying_descendants"))),
      ("nr_descendants", Err(Missing("value"))),
      ("nr_descendants -1\nnr_dying_descendants 0", Err(Invalid("nr_descendants", "-1"))),
      ("nr_descendants 0\nnr_dying_descendants nope", Err(Invalid("nr_dying_descendants", "nope"))),
      ("nr_descendants 0\nnr_dying_descendants 0\nnr_subsys_memory nope", Err(Invalid("nr_subsys_", "nope"))),
      (
        "nr_descendants 0\nnr_dying_descendants 0\nnr_dying_subsys_memory 18446744073709551616",
        Err(Invalid("nr_dying_subsys_", "18446744073709551616")),
      ),
      ("nr_descendants 0\nnr_dying_descendants 0\nnr_subsys_ 1", Err(Invalid("controller", ""))),
      ("nr_descendants 0 extra", Err(Excess)),
    ]);
  }
}
