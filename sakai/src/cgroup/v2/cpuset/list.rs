use std::{ops::RangeInclusive, str::FromStr};

use crate::error::{ParseError, ParseValueError};

/// Shared storage for the kernel's ascending, non-overlapping decimal ID ranges.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct IdList {
  ranges: Vec<RangeInclusive<u32>>,
}

impl IdList {
  fn parse(raw: &str, field: &'static str) -> Result<Self, ParseError<ParseValueError>> {
    let value = raw.trim();
    let ranges = match value.is_empty() {
      | true => Vec::new(),
      | false => value
        .split(',')
        .map(|part| {
          let invalid = |source| ParseError::invalid(raw, field, part, source);
          let number = |text: &str| match !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
            | true => text.parse::<u32>().map_err(|error| invalid(error.into())),
            | false => Err(invalid(ParseValueError::OutOfRange)),
          };
          let (start, end) = match part.split_once('-') {
            | Some((start, end)) => (number(start)?, number(end)?),
            | None => {
              let id = number(part)?;
              (id, id)
            },
          };
          match start <= end {
            | true => Ok(start..=end),
            | false => Err(invalid(ParseValueError::OutOfRange)),
          }
        })
        .collect::<Result<Vec<_>, _>>()?,
    };
    match ranges.iter().zip(ranges.iter().skip(1)).all(|(previous, next)| previous.end() < next.start()) {
      | true => Ok(Self { ranges }),
      | false => Err(ParseError::invalid(raw, field, value, ParseValueError::OutOfRange)),
    }
  }
}

macro_rules! placement_list {
  ($(#[$doc:meta])* $name:ident, $field:literal, $configured:ident, $file:literal, $effective:literal) => {
    $(#[$doc])*
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct $name {
      list: IdList,
    }

    impl $name {
      /// The configured placement interface filename; absent at the hierarchy root.
      /// See [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
      pub const $configured: &'static str = $file;

      /// The effective placement interface filename, including at the hierarchy root.
      /// See [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
      pub const EFFECTIVE_FILE_NAME: &'static str = $effective;

      /// Inspects the stored ascending, non-overlapping inclusive ID ranges.
      ///
      /// Single IDs have equal endpoints. Storage is proportional to the number
      /// of ranges, not the number of IDs. This getter performs no kernel I/O.
      #[must_use]
      pub fn ranges(&self) -> &[RangeInclusive<u32>] { &self.list.ranges }

      /// Iterates the stored IDs in ascending order without allocating an expanded list.
      ///
      /// This inspects the snapshot only. Iterating a large range can take a long time.
      pub fn ids(&self) -> impl Iterator<Item = u32> + '_ { self.list.ranges.iter().cloned().flatten() }

      /// Tests membership in the stored snapshot without kernel I/O.
      #[must_use]
      pub fn contains(&self, id: u32) -> bool { self.list.ranges.iter().any(|range| range.contains(&id)) }
    }

    impl FromStr for $name {
      type Err = ParseError<ParseValueError>;

      fn from_str(raw: &str) -> Result<Self, Self::Err> { Ok(Self { list: IdList::parse(raw, $field)? }) }
    }
  };
}

placement_list!(
  /// A snapshot of configured or effective CPU IDs for tasks in one cgroup.
  ///
  /// IDs are dimensionless `u32` identifiers, not CPU counts. Parsing accepts
  /// comma-separated decimal IDs and inclusive ranges, in ascending order
  /// without overlap; an empty file is valid. Configured emptiness means
  /// inheritance from the nearest nonempty ancestor (or all available CPUs).
  /// Effective emptiness means no grant. Effective CPUs can differ from the
  /// request, including falling back to the parent's grant if none of the
  /// requested CPUs can be granted. Configuration changes affect either list;
  /// hotplug affects only the effective list. These values are not per-thread
  /// affinity masks and do not resolve ancestor CPU bandwidth limits.
  /// See [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
  ///
  /// ```
  /// use sakai::v2::cpuset::CpuList;
  /// let cpus: CpuList = "0-2,5".parse()?;
  /// assert_eq!(cpus.ids().collect::<Vec<_>>(), vec![0, 1, 2, 5]);
  /// assert!(!cpus.contains(3));
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  CpuList,
  "cpus",
  CPUS_FILE_NAME,
  "cpuset.cpus",
  "cpuset.cpus.effective"
);

placement_list!(
  /// A snapshot of configured or effective NUMA memory-node IDs for one cgroup.
  ///
  /// IDs are dimensionless `u32` identifiers, not bytes or node counts. Parsing
  /// accepts comma-separated decimal IDs and inclusive ranges, in ascending
  /// order without overlap; an empty file is valid. Configured emptiness means
  /// inheritance from the nearest nonempty ancestor (or all available nodes).
  /// Effective emptiness means no grant. Effective nodes can differ from the
  /// request, including falling back to the parent's grant if none of the
  /// requested nodes can be granted. Configuration changes affect either list;
  /// hotplug affects only the effective list. Reading does not migrate memory
  /// or report where existing pages reside; see `memory.numa_stat` for usage.
  /// See [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
  MemoryNodeList,
  "mems",
  MEMS_FILE_NAME,
  "cpuset.mems",
  "cpuset.mems.effective"
);

#[cfg(test)]
mod tests {
  use assertables::assert_ok;

  use super::*;
  use crate::parse::tests::{Cases, Failure::Invalid};

  #[test]
  fn parses_placement_lists_and_rejects_malformed_ranges() {
    for (raw, expected) in [
      ("", vec![]),
      (" \n", vec![]),
      ("0\n", std::iter::once(0..=0).collect()),
      (include_str!("../fixtures/p4/cpuset.cpus"), vec![0..=3, 5..=5, 7..=9]),
      (include_str!("../fixtures/p4/cpuset.mems"), vec![0..=1, 3..=3]),
      (include_str!("../fixtures/p4_older/cpuset.cpus"), vec![]),
      (" 0-3,5,7-9\n", vec![0..=3, 5..=5, 7..=9]),
      ("1,2-2,3", vec![1..=1, 2..=2, 3..=3]),
      ("0-4294967295", std::iter::once(0..=u32::MAX).collect()),
      ("4294967295", std::iter::once(u32::MAX..=u32::MAX).collect()),
    ] {
      Cases::<CpuList>::check([(raw, Ok(CpuList { list: IdList { ranges: expected.clone() } }))]);
      Cases::<MemoryNodeList>::check([(raw, Ok(MemoryNodeList { list: IdList { ranges: expected } }))]);
    }
    for (raw, bad) in [
      (",", ""),
      ("1,", ""),
      (",1", ""),
      ("1,,2", ""),
      ("-1", "-1"),
      ("+1", "+1"),
      ("1-", "1-"),
      ("1--2", "1--2"),
      ("3-1", "3-1"),
      ("1-2-3", "1-2-3"),
      ("1 2", "1 2"),
      ("1, 2", " 2"),
      ("1 - 2", "1 - 2"),
      ("max", "max"),
      ("4294967296", "4294967296"),
      ("0-4294967296", "0-4294967296"),
      ("2,1", "2,1"),
      ("1,1", "1,1"),
      ("0-2,2-3", "0-2,2-3"),
    ] {
      Cases::<CpuList>::check([(raw, Err(Invalid("cpus", bad)))]);
      Cases::<MemoryNodeList>::check([(raw, Err(Invalid("mems", bad)))]);
    }
  }

  #[test]
  fn inspects_stored_ids_without_expanding_ranges() {
    let cpus = assert_ok!("0-2,5,4294967295".parse::<CpuList>());
    assert_eq!(cpus.ids().collect::<Vec<_>>(), vec![0, 1, 2, 5, u32::MAX]);
    for (id, expected) in [(0, true), (2, true), (3, false), (5, true), (u32::MAX, true)] {
      assert_eq!(cpus.contains(id), expected);
    }
    let nodes = assert_ok!("0-1,4".parse::<MemoryNodeList>());
    assert_eq!(nodes.ids().collect::<Vec<_>>(), vec![0, 1, 4]);
    assert!(nodes.contains(4));
    assert!(!nodes.contains(3));
    assert_eq!(CpuList::default().ids().next(), None);
    assert!(!MemoryNodeList::default().contains(0));
  }
}
