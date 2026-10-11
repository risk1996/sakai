//! Miscellaneous scalar-resource accounting and limits.
//!
//! Resource names are kernel-defined, hardware-dependent identifiers such as
//! `sev` and `sev_es`. Values count resource units, never bytes. Empty files
//! mean no resources were reported; absence of a resource is distinct from zero.
//! See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).

use std::{collections::BTreeMap, str::FromStr};

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::Error;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{KeyedFields, ParseCgroup, ParseCount},
  unit::Count,
};

/// A borrowed miscellaneous-resource reader for one pinned cgroup.
///
/// Every method performs fresh read-only I/O; separate calls are not atomic.
/// Disabled controllers, unsupported kernels and root exemptions return
/// [`crate::Error::FileMissing`]. See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Misc<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Misc<'_> {
  /// Machine-wide resource capacities; available only at the hierarchy root.
  ///
  /// Reads a fresh [`MiscCapacity`] snapshot. See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn capacity(&self) -> Result<MiscCapacity, Error> { self.cgroup.parse(MiscCapacity::FILE_NAME) }

  /// Hierarchical resource usage in counts; older kernels omit the root file.
  ///
  /// Reads a fresh [`MiscCurrent`] snapshot. See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn current(&self) -> Result<MiscCurrent, Error> { self.cgroup.parse(MiscCurrent::FILE_NAME) }

  /// Historical hierarchical maximum resource usage; since Linux 6.11.
  ///
  /// Reads a fresh [`MiscPeak`] snapshot. See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn peak(&self) -> Result<MiscPeak, Error> { self.cgroup.parse(MiscPeak::FILE_NAME) }

  /// Configured resource limits in counts; absent at the hierarchy root.
  ///
  /// Reads a fresh [`MiscMax`] snapshot. See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max(&self) -> Result<MiscMax, Error> { self.cgroup.parse(MiscMax::FILE_NAME) }

  /// Hierarchical resource limit failures; absent at the hierarchy root.
  ///
  /// Reads a fresh [`MiscEvents`] snapshot. See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn events(&self) -> Result<MiscEvents, Error> { self.cgroup.parse(MiscEvents::FILE_NAME) }

  /// Failures at this cgroup’s own limit; since Linux 6.11, absent at the root.
  ///
  /// Reads a fresh [`MiscEvents`] snapshot. See [Linux misc interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#misc).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn events_local(&self) -> Result<MiscEvents, Error> { self.cgroup.parse(MiscEvents::LOCAL_FILE_NAME) }
}

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

impl MiscCapacity {
  /// The cgroup v2 `misc.capacity` interface filename.
  pub const FILE_NAME: &'static str = "misc.capacity";

  /// Reported resource names and their counts of resource units.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  #[must_use]
  pub fn resources(&self) -> &BTreeMap<String, Count> { &self.resources }
}

impl FromStr for MiscCapacity {
  type Err = ParseError<ParseValueError>;

  fn from_str(raw: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(raw, |_| "value")?;
    let resources = fields
      .iter()
      .map(|(name, value)| Ok((name.to_owned(), <Count as ParseCgroup<ParseCount>>::parse_field(raw, "value", value)?)))
      .collect::<Result<_, Self::Err>>()?;
    Ok(Self { resources })
  }
}

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

impl MiscCurrent {
  /// The cgroup v2 `misc.current` interface filename.
  pub const FILE_NAME: &'static str = "misc.current";

  /// Reported resource names and their counts of resource units.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  #[must_use]
  pub fn resources(&self) -> &BTreeMap<String, Count> { &self.resources }
}

impl FromStr for MiscCurrent {
  type Err = ParseError<ParseValueError>;

  fn from_str(raw: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(raw, |_| "value")?;
    let resources = fields
      .iter()
      .map(|(name, value)| Ok((name.to_owned(), <Count as ParseCgroup<ParseCount>>::parse_field(raw, "value", value)?)))
      .collect::<Result<_, Self::Err>>()?;
    Ok(Self { resources })
  }
}

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

impl MiscPeak {
  /// The cgroup v2 `misc.peak` interface filename.
  pub const FILE_NAME: &'static str = "misc.peak";

  /// Reported resource names and their counts of resource units.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  #[must_use]
  pub fn resources(&self) -> &BTreeMap<String, Count> { &self.resources }
}

impl FromStr for MiscPeak {
  type Err = ParseError<ParseValueError>;

  fn from_str(raw: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(raw, |_| "value")?;
    let resources = fields
      .iter()
      .map(|(name, value)| Ok((name.to_owned(), <Count as ParseCgroup<ParseCount>>::parse_field(raw, "value", value)?)))
      .collect::<Result<_, Self::Err>>()?;
    Ok(Self { resources })
  }
}

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
  resources: BTreeMap<String, MaxOr<Count>>,
}

impl MiscMax {
  /// The cgroup v2 `misc.max` interface filename.
  pub const FILE_NAME: &'static str = "misc.max";

  /// Reported resource names and their configured limits in resource counts.
  ///
  /// Inspects this stored snapshot without I/O. Omitted resources are absent.
  #[must_use]
  pub fn resources(&self) -> &BTreeMap<String, MaxOr<Count>> { &self.resources }
}

impl FromStr for MiscMax {
  type Err = ParseError<ParseValueError>;

  fn from_str(raw: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(raw, |_| "value")?;
    let resources = fields
      .iter()
      .map(|(name, value)| {
        Ok((name.to_owned(), <MaxOr<Count> as ParseCgroup<ParseCount>>::parse_field(raw, "value", value)?))
      })
      .collect::<Result<_, Self::Err>>()?;
    Ok(Self { resources })
  }
}

/// Resource limit failures from `misc.events` or `misc.events.local`.
///
/// `misc.events` aggregates limit-hit locations in this subtree. Local events
/// count failures at this cgroup’s own limit, including descendant allocations;
/// they exclude events propagated from descendant limits. A subtree allocation
/// rejected at an ancestor’s limit is counted at that ancestor.
/// Counts are volatile, cumulative failure events, not resource usage. Only
/// `<resource>.max` fields are interpreted; unknown future event kinds are ignored.
/// Since Linux 5.16 for events, and Linux 6.11 for local events. See
/// [Linux 5.16 source](https://github.com/torvalds/linux/blob/v5.16/kernel/cgroup/misc.c) and
/// [Linux 6.11 source](https://github.com/torvalds/linux/blob/v6.11/kernel/cgroup/misc.c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiscEvents {
  resources: BTreeMap<String, Count>,
}

impl MiscEvents {
  /// Hierarchical miscellaneous-resource event interface filename.
  pub const FILE_NAME: &'static str = "misc.events";
  /// Local miscellaneous-resource event interface filename.
  pub const LOCAL_FILE_NAME: &'static str = "misc.events.local";

  /// Stored resource names mapped to cumulative max-boundary failure counts.
  ///
  /// A missing resource is absent, not a measured zero. Performs no I/O.
  #[must_use]
  pub fn resources(&self) -> &BTreeMap<String, Count> { &self.resources }
}

impl FromStr for MiscEvents {
  type Err = ParseError<ParseValueError>;

  fn from_str(raw: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(raw, |_| "value")?;
    let resources = fields
      .iter()
      .filter_map(|(key, value)| key.strip_suffix(".max").map(|name| (name, value)))
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

#[cfg(test)]
mod tests {
  use assertables::assert_ok;

  use super::*;
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
      let reader = group.misc();
      for (name, result) in [
        (MiscCapacity::FILE_NAME, reader.capacity().map(|_| ())),
        (MiscCurrent::FILE_NAME, reader.current().map(|_| ())),
        (MiscPeak::FILE_NAME, reader.peak().map(|_| ())),
        (MiscMax::FILE_NAME, reader.max().map(|_| ())),
        (MiscEvents::FILE_NAME, reader.events().map(|_| ())),
        (MiscEvents::LOCAL_FILE_NAME, reader.events_local().map(|_| ())),
      ] {
        if group.path().join(name).exists() {
          assert_ok!(result);
        } else {
          assert!(matches!(result, Err(Error::FileMissing { path }) if path == group.path().join(name)));
        }
      }
    }
  }
}
