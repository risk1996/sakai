use std::{fmt::Write, ops::RangeInclusive, str::FromStr};

use crate::error::{ParseError, ParseValueError};

const RANGE_SEPARATOR: char = '-';
const LIST_SEPARATOR: char = ',';

fn valid_ranges(ranges: &[RangeInclusive<u32>]) -> bool {
  ranges.iter().all(|range| !range.is_empty())
    && ranges.iter().zip(ranges.iter().skip(1)).all(|(previous, next)| previous.end() < next.start())
}

fn parse_ranges(raw: &str, field: &'static str) -> Result<Vec<RangeInclusive<u32>>, ParseError<ParseValueError>> {
  let value = raw.trim();
  let ranges = match value.is_empty() {
    | true => Vec::new(),
    | false => value
      .split(LIST_SEPARATOR)
      .map(|part| {
        let invalid = |source| ParseError::invalid(raw, field, part, source);
        let number = |text: &str| match !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
          | true => text.parse::<u32>().map_err(|error| invalid(error.into())),
          | false => Err(invalid(ParseValueError::OutOfRange)),
        };
        let (start, end) = match part.split_once(RANGE_SEPARATOR) {
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
  match valid_ranges(&ranges) {
    | true => Ok(ranges),
    | false => Err(ParseError::invalid(raw, field, value, ParseValueError::OutOfRange)),
  }
}

macro_rules! placement_list {
  ($(#[$doc:meta])* $name:ident, $field:literal) => {
    $(#[$doc])*
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    #[non_exhaustive]
    pub struct $name {
      /// Inclusive ID ranges, in ascending order without overlap.
      ///
      /// Single IDs have equal endpoints. Storage is proportional to the number
      /// of ranges, not the number of IDs.
      pub ranges: Vec<RangeInclusive<u32>>,
    }

    impl $name {
      /// Iterates the stored IDs in ascending order without allocating an expanded list.
      ///
      /// This inspects the snapshot only. Iterating a large range can take a long time.
      pub fn ids(&self) -> impl Iterator<Item = u32> + '_ { self.ranges.iter().cloned().flatten() }

      /// Tests membership in the stored snapshot without kernel I/O.
      #[must_use]
      pub fn contains(&self, id: u32) -> bool { self.ranges.iter().any(|range| range.contains(&id)) }
    }

    impl $name {
      /// Encodes the stored IDs as a comma-separated list of IDs and ranges.
      ///
      /// # Errors
      ///
      /// Returns an error if a range is empty or the ranges are out of order or overlap.
      pub fn encode_cgroup(&self) -> Result<String, crate::error::EncodeError> {
        if !valid_ranges(&self.ranges) {
          return Err(crate::error::EncodeError { field: $field });
        }
        let mut output = String::new();
        for range in &self.ranges {
          if !output.is_empty() {
            output.push(LIST_SEPARATOR);
          }
          if range.start() == range.end() {
            write!(output, "{}", range.start()).map_err(|_error| crate::error::EncodeError { field: $field })?;
          } else {
            write!(output, "{}{RANGE_SEPARATOR}{}", range.start(), range.end())
              .map_err(|_error| crate::error::EncodeError { field: $field })?;
          }
        }
        Ok(output)
      }
    }

    impl FromStr for $name {
      type Err = ParseError<ParseValueError>;

      fn from_str(raw: &str) -> Result<Self, Self::Err> { Ok(Self { ranges: parse_ranges(raw, $field)? }) }
    }
  };
}

placement_list!(
  /// A snapshot of CPU IDs from a cgroup's placement or partition interfaces.
  ///
  /// IDs are dimensionless integers. Empty lists and the meaning of
  /// configured versus effective placement depend on the kernel interface.
  ///
  /// ```
  /// use sakai::cgroup::common::CpuList;
  /// let cpus: CpuList = "0-2,5".parse()?;
  /// assert_eq!(cpus.ids().collect::<Vec<_>>(), vec![0, 1, 2, 5]);
  /// assert!(!cpus.contains(3));
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  CpuList,
  "cpus"
);

placement_list!(
  /// A snapshot of configured or effective NUMA memory-node IDs for one cgroup.
  ///
  /// IDs are dimensionless integers. This snapshot describes placement,
  /// independent of where the cgroup's existing pages reside.
  MemoryNodeList,
  "mems"
);

pub(in crate::cgroup) fn placement_command(
  command: Result<String, crate::error::EncodeError>,
) -> Result<String, crate::error::EncodeError> {
  let mut command = command?;
  if command.is_empty() {
    command.push('\n');
  }
  Ok(command)
}
