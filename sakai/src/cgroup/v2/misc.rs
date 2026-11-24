//! Miscellaneous scalar-resource accounting and limits.
//!
//! Resource names are kernel-defined, hardware-dependent identifiers such as
//! `sev` and `sev_es`. Values count resource units, never bytes. Empty files
//! mean no resources were reported; absence of a resource is distinct from zero.
//! See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).

use std::{collections::BTreeMap, fmt::Write, str::FromStr};

use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{KeyedFields, ParseCgroup, ParseCount, cgroup_value},
  unit::Count,
};

/// Machine-wide available miscellaneous resource capacities.
///
/// Values count resource units keyed by kernel resource names. This stored
/// snapshot can change on the next read; getters perform no I/O. Empty maps
/// are valid; hardware-dependent and future resource names are preserved.
/// See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiscCapacity {
  resources: BTreeMap<String, Count>,
}

crate::v2::interface::cgroup_file!(MiscCapacity, "misc.capacity", Read);

impl MiscCapacity {
  /// The cgroup v2 `misc.capacity` interface filename.
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;

  /// Reported resource names and their counts of resource units.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  #[must_use]
  pub fn resources(&self) -> &BTreeMap<String, Count> { &self.resources }
}

cgroup_value! { map MiscCapacity, resources: ParseCount, "value" }

/// Current hierarchical miscellaneous resource usage.
///
/// Values count resource units keyed by kernel resource names. This stored
/// snapshot can change on the next read; getters perform no I/O. Empty maps
/// are valid; hardware-dependent and future resource names are preserved.
/// Since Linux 6.5, this file also exists at the root; earlier kernels omit it. See
/// [Linux 6.5 source](https://github.com/torvalds/linux/blob/v6.5/kernel/cgroup/misc.c).
/// See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiscCurrent {
  resources: BTreeMap<String, Count>,
}

crate::v2::interface::cgroup_file!(MiscCurrent, "misc.current", Read);

impl MiscCurrent {
  /// The cgroup v2 `misc.current` interface filename.
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;

  /// Reported resource names and their counts of resource units.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  #[must_use]
  pub fn resources(&self) -> &BTreeMap<String, Count> { &self.resources }
}

cgroup_value! { map MiscCurrent, resources: ParseCount, "value" }

/// Historical maximum hierarchical miscellaneous resource usage.
///
/// Values count resource units keyed by kernel resource names. This stored
/// snapshot can change on the next read; getters perform no I/O. Empty maps
/// are valid; hardware-dependent and future resource names are preserved.
/// Peak values cover the cgroup lifetime and can include transient charges
/// later canceled by an ancestor limit. Reading never resets the watermark.
/// Since Linux 6.11. See [Linux 6.11 source](https://github.com/torvalds/linux/blob/v6.11/kernel/cgroup/misc.c).
/// See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiscPeak {
  resources: BTreeMap<String, Count>,
}

crate::v2::interface::cgroup_file!(MiscPeak, "misc.peak", Read);

impl MiscPeak {
  /// The cgroup v2 `misc.peak` interface filename.
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;

  /// Reported resource names and their counts of resource units.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  #[must_use]
  pub fn resources(&self) -> &BTreeMap<String, Count> { &self.resources }
}

cgroup_value! { map MiscPeak, resources: ParseCount, "value" }

/// Configured hierarchical miscellaneous resource limits.
///
/// Values count resource units keyed by kernel resource names. This stored
/// snapshot can change on the next read; getters perform no I/O. Empty maps
/// are valid; hardware-dependent and future resource names are preserved.
/// `max` means unlimited here; ancestor limits and machine capacity still apply.
/// Zero is valid, and a limit may exceed the separately read machine capacity.
/// See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiscMax {
  /// Maximum counts per resource.
  pub resources: BTreeMap<String, MaxOr<Count>>,
}

crate::v2::interface::cgroup_file!(file MiscMax, "misc.max", ReadWrite);
crate::v2::interface::cgroup_file!(read MiscMax => MiscMax);

impl MiscMax {
  /// The cgroup v2 `misc.max` interface filename.
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;

  /// Reported resource names and their configured limits in resource counts.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  #[must_use]
  pub fn resources(&self) -> &BTreeMap<String, MaxOr<Count>> { &self.resources }
}

cgroup_value! { map MiscMax, resources: ParseCount, "value" }

/// Resource limit or platform-capacity failures from `misc.events` or `misc.events.local`.
///
/// `misc.events` aggregates limit-hit locations in this subtree. Local events
/// count failures at this cgroup's own limit or the platform capacity checked
/// at this cgroup, including descendant allocations;
/// they exclude events propagated from descendant limits. A subtree allocation
/// rejected at an ancestor's limit or capacity check is counted at that ancestor.
/// Counts are volatile, cumulative failure events, not resource usage. Only
/// `<resource>.max` fields are interpreted; unknown future event kinds are ignored.
/// Since Linux 5.16 for events, and Linux 6.11 for local events. See
/// [Linux 5.16 source](https://github.com/torvalds/linux/blob/v5.16/kernel/cgroup/misc.c) and
/// [Linux 6.11 source](https://github.com/torvalds/linux/blob/v6.11/kernel/cgroup/misc.c).
/// The implementation checks capacity alongside each cgroup limit; see
/// [Linux charging](https://github.com/torvalds/linux/blob/master/kernel/cgroup/misc.c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiscEvents {
  resources: BTreeMap<String, Count>,
}

crate::v2::interface::cgroup_file!(MiscEvents, "misc.events", Read);

impl MiscEvents {
  /// Hierarchical miscellaneous-resource event interface filename.
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;
  /// Local miscellaneous-resource event interface filename.
  pub const LOCAL_FILE_NAME: &'static str = "misc.events.local";

  /// Stored resource names mapped to cumulative limit or capacity failure counts.
  ///
  /// A missing resource is absent, not a measured zero. Performs no I/O.
  #[must_use]
  pub fn resources(&self) -> &BTreeMap<String, Count> { &self.resources }
}

const EVENT_SUFFIX: &str = ".max";

impl MiscEvents {
  /// Encodes the stored value as canonical cgroup text.
  ///
  /// # Errors
  ///
  /// Returns an error if a field cannot be represented exactly.
  pub fn encode_cgroup(&self) -> Result<String, crate::error::EncodeError> {
    let mut output = String::new();
    for (name, value) in &self.resources {
      if !output.is_empty() {
        output.push('\n');
      }
      write!(output, "{name}{EVENT_SUFFIX} {}", value.value)
        .map_err(|_error| crate::error::EncodeError { field: "max" })?;
    }
    Ok(output)
  }
}

impl FromStr for MiscEvents {
  type Err = ParseError<ParseValueError>;

  fn from_str(raw: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(raw, |_| "value")?;
    let resources = fields
      .iter()
      .filter_map(|(key, value)| key.strip_suffix(EVENT_SUFFIX).map(|name| (name, value)))
      .map(|(name, value)| {
        if name.is_empty() {
          return Err(ParseError::invalid(raw, "resource", ".max", ParseValueError::OutOfRange));
        }
        Ok((name.to_owned(), <Count as ParseCgroup<ParseCount>>::parse_field(raw, "max", value)?))
      })
      .collect::<Result<_, Self::Err>>()?;
    Ok(Self { resources })
  }
}

crate::v2::interface::cgroup_file!(alias
  /// Local miscellaneous resource events.
  MiscEventsLocal, MiscEvents::LOCAL_FILE_NAME, Read, MiscEvents
);

crate::v2::interface::cgroup_file!(write MiscMax, 'a => (&'a str, MaxOr<Count>), |(resource, value)| {
  super::interface::encode_keyed::<ParseCount>(resource, "resource", &value, "limit")
});

#[cfg(test)]
mod tests {
  use assertables::assert_ok;

  use super::*;
  #[cfg(target_os = "linux")]
  use crate::Cgroup;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

  #[test]
  fn parses_named_resources_limits_and_event_suffixes() {
    let count = |value| Count { value, ..Default::default() };
    Cases::<MiscCurrent>::check([
      (
        include_str!("fixtures/p6_misc/misc.current"),
        Ok(MiscCurrent {
          resources: BTreeMap::from([
            ("sev".into(), count(4)),
            ("sev_es".into(), count(0)),
            ("future_resource".into(), count(9)),
          ]),
        }),
      ),
      ("", Ok(MiscCurrent { resources: BTreeMap::new() })),
      ("sev 18446744073709551615", Ok(MiscCurrent { resources: BTreeMap::from([("sev".into(), count(u64::MAX))]) })),
      ("sev -1", Err(Invalid("value", "-1"))),
      ("sev max", Err(Invalid("value", "max"))),
      ("sev 18446744073709551616", Err(Invalid("value", "18446744073709551616"))),
      ("sev", Err(Missing("value"))),
      ("sev 1 extra", Err(Excess)),
    ]);
    Cases::<MiscMax>::check([
      (
        include_str!("fixtures/p6_misc/misc.max"),
        Ok(MiscMax {
          resources: BTreeMap::from([("sev".into(), MaxOr::Value(count(0))), ("sev_es".into(), MaxOr::Max)]),
        }),
      ),
      ("", Ok(MiscMax { resources: BTreeMap::new() })),
      ("sev nope", Err(Invalid("value", "nope"))),
    ]);
    Cases::<MiscEvents>::check([
      (
        include_str!("fixtures/p6_misc/misc.events"),
        Ok(MiscEvents { resources: BTreeMap::from([("sev".into(), count(3)), ("sev_es".into(), count(0))]) }),
      ),
      ("", Ok(MiscEvents { resources: BTreeMap::new() })),
      ("sev.future nope", Ok(MiscEvents { resources: BTreeMap::new() })),
      ("sev.max -1", Err(Invalid("max", "-1"))),
      (".max 0", Err(Invalid("resource", ".max"))),
    ]);
    assert_eq!(assert_ok!("sev 50".parse::<MiscCapacity>()), MiscCapacity {
      resources: BTreeMap::from([("sev".into(), count(50))])
    });
    assert_eq!(assert_ok!("sev 10".parse::<MiscPeak>()), MiscPeak {
      resources: BTreeMap::from([("sev".into(), count(10))])
    });
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn reads_supported_empty_and_missing_interfaces() {
    for fixture in ["p6_misc", "p6_misc_root", "p6_misc_older", "p6_misc_older_root"] {
      let group = Cgroup::fixture(&format!("fixtures/{fixture}"));
      group.check_fixture(MiscCapacity::FILE_NAME, true, || group.read_value::<MiscCapacity>());
      group.check_fixture(MiscCurrent::FILE_NAME, true, || group.read_value::<MiscCurrent>());
      group.check_fixture(MiscPeak::FILE_NAME, true, || group.read_value::<MiscPeak>());
      group.check_fixture(MiscMax::FILE_NAME, true, || group.read_value::<MiscMax>());
      group.check_fixture(MiscEvents::FILE_NAME, true, || group.read_value::<MiscEvents>());
      group.check_fixture(MiscEvents::LOCAL_FILE_NAME, true, || group.read_value::<MiscEventsLocal>());
    }
  }
}
