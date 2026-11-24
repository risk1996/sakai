//! HugeTLB usage, reservations, limits, events, and NUMA placement by page size.
//!
//! HugeTLB pages are distinct from transparent huge pages. Each supported page
//! size has its own interfaces; reads do not allocate pages or reservations.

use std::{collections::BTreeMap, fmt, fmt::Write, str::FromStr};

use nutype::nutype;
use uom::si::information::byte;

use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{EncodeCgroup, KeyedFields, ParseBytes, ParseCgroup, ParseCount, cgroup_value},
  unit::{Bytes, Count},
};

/// A HugeTLB page size expressed in bytes, independent of the system page size.
///
/// `try_new` accepts powers of two of at least 1 KiB; this validates the name,
/// not whether the running kernel supports that size. Parse kernel labels such
/// as `64KB`, `2MB`, or `1GB`, whose suffixes denote binary units. Labels must
/// use the kernel's canonical unit and decimal spelling. `AsRef` and
/// `into_inner` expose the byte quantity.
///
/// ```
/// use sakai::v2::hugetlb::PageSize;
/// use uom::si::information::byte;
///
/// let size = "2MB".parse::<PageSize>()?;
/// assert_eq!(size.as_ref().get::<byte>(), 2_097_152);
/// assert_eq!(size.to_string(), "2MB");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// See [Linux HugeTLB filenames](https://github.com/torvalds/linux/blob/v5.15/mm/hugetlb_cgroup.c).
#[nutype(
  validate(predicate = |size| size.get::<byte>() >= 1024 && size.get::<byte>().is_power_of_two()),
  derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, AsRef)
)]
pub struct PageSize(Bytes);

cgroup_value! { display PageSize }

impl PageSize {
  const GIBIBYTE: u64 = 1 << 30;
  const KIBIBYTE: u64 = 1 << 10;
  const MEBIBYTE: u64 = 1 << 20;

  fn file_name(&self, suffix: &str) -> String { format!("hugetlb.{self}.{suffix}") }
}

impl fmt::Display for PageSize {
  #[expect(clippy::arithmetic_side_effects, reason = "selected binary unit divisors are positive constants")]
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let bytes = self.as_ref().get::<byte>();
    let (unit, suffix) = if bytes >= Self::GIBIBYTE {
      (Self::GIBIBYTE, "GB")
    } else if bytes >= Self::MEBIBYTE {
      (Self::MEBIBYTE, "MB")
    } else {
      (Self::KIBIBYTE, "KB")
    };
    write!(formatter, "{}{suffix}", bytes / unit)
  }
}

impl FromStr for PageSize {
  type Err = ParseError<ParseValueError>;

  fn from_str(label: &str) -> Result<Self, Self::Err> {
    let invalid = || ParseError::invalid(label, "page_size", label, ParseValueError::OutOfRange);
    let (number, unit) = if let Some(number) = label.strip_suffix("GB") {
      (number, Self::GIBIBYTE)
    } else if let Some(number) = label.strip_suffix("MB") {
      (number, Self::MEBIBYTE)
    } else if let Some(number) = label.strip_suffix("KB") {
      (number, Self::KIBIBYTE)
    } else {
      return Err(invalid());
    };
    let bytes = number.parse::<u64>().ok().and_then(|number| number.checked_mul(unit)).ok_or_else(invalid)?;
    let size = Self::try_new(Bytes::new::<byte>(bytes)).map_err(|_error| invalid())?;
    if size.to_string() != label {
      return Err(invalid());
    }
    Ok(size)
  }
}

macro_rules! scalar {
  ($name:ident, $value:ty, $suffix:literal, $field:literal, $documentation:literal) => {
    #[doc = $documentation]
    ///
    /// Values can change between reads. Getters inspect the stored snapshot.
    /// See [Linux HugeTLB accounting](https://docs.kernel.org/admin-guide/cgroup-v1/hugetlb.html)
    /// and [v2 source](https://github.com/torvalds/linux/blob/v5.15/mm/hugetlb_cgroup.c).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct $name {
      /// Stored quantity or limit in bytes.
      pub value: $value,
    }

    impl $name {
      /// Interface filename template; replace `<hugepagesize>` with a [`PageSize`] label.
      pub const FILE_NAME_TEMPLATE: &'static str = concat!("hugetlb.<hugepagesize>.", $suffix);
      const SUFFIX: &'static str = $suffix;

      /// Returns the stored byte quantity or limit without performing I/O.
      #[must_use]
      pub const fn value(&self) -> $value { self.value }
    }

    cgroup_value! { scalar $name, value: ParseBytes, $field }
  };
}

scalar!(HugetlbCurrent, Bytes, "current", "current", "Hierarchical faulted HugeTLB usage in bytes for one page size.");
scalar!(
  HugetlbMax,
  MaxOr<Bytes>,
  "max",
  "max",
  "Configured HugeTLB fault limit in bytes, or `max`; ancestors can still constrain usage."
);
scalar!(
  HugetlbReservedCurrent,
  Bytes,
  "rsvd.current",
  "current",
  "Hierarchical reservation-accounting usage in bytes: reserved pages plus faulted pages without prior reservations, \
   not only unused reservations."
);
scalar!(
  HugetlbReservedMax,
  MaxOr<Bytes>,
  "rsvd.max",
  "max",
  "Configured reservation-accounting limit in bytes, or `max`; ancestors can still constrain reservations and \
   unreserved faults."
);

/// Cumulative HugeTLB allocation-failure event counts for one page size.
///
/// `events` includes this cgroup and descendants; `events.local` includes only
/// this cgroup. Both fault and reservation limits contribute to the shared
/// `max` counter. Counts are volatile; unknown future keys are ignored.
///
/// See [Linux HugeTLB events](https://docs.kernel.org/admin-guide/cgroup-v2.html#hugetlb)
/// and [reservation charge implementation](https://github.com/torvalds/linux/blob/v5.15/mm/hugetlb_cgroup.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HugetlbEvents {
  max: Count,
}

impl HugetlbEvents {
  /// Hierarchical event filename template, parameterized by [`PageSize`].
  pub const FILE_NAME_TEMPLATE: &'static str = "hugetlb.<hugepagesize>.events";
  /// Local event filename template, parameterized by [`PageSize`].
  pub const LOCAL_FILE_NAME_TEMPLATE: &'static str = "hugetlb.<hugepagesize>.events.local";
  const LOCAL_SUFFIX: &'static str = "events.local";
  const SUFFIX: &'static str = "events";

  /// Returns the stored count of allocation failures caused by a HugeTLB limit.
  #[must_use]
  pub const fn max(&self) -> Count { self.max }
}

cgroup_value! { keyed HugetlbEvents, |_| "value", { max: required ParseCount => "max" } }

/// Hierarchical in-use HugeTLB bytes and NUMA distribution for one page size.
///
/// Since Linux 5.17. Only active, in-use pages are included, not unused
/// reservations. Total and node counters are read independently by the kernel;
/// the node sum need not equal the total while usage changes. Unknown future
/// keys are ignored. Node IDs are unsigned integers and values are bytes.
///
/// See [Linux 5.17 source](https://github.com/torvalds/linux/blob/v5.17/mm/hugetlb_cgroup.c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HugetlbNumaStat {
  total: Bytes,
  nodes: BTreeMap<u32, Bytes>,
}

impl HugetlbNumaStat {
  /// NUMA filename template, parameterized by [`PageSize`].
  pub const FILE_NAME_TEMPLATE: &'static str = "hugetlb.<hugepagesize>.numa_stat";
  const SUFFIX: &'static str = "numa_stat";

  /// Returns stored hierarchical usage in bytes across all NUMA nodes.
  #[must_use]
  pub const fn total(&self) -> Bytes { self.total }

  /// Returns stored in-use bytes keyed by NUMA node ID.
  #[must_use]
  pub const fn nodes(&self) -> &BTreeMap<u32, Bytes> { &self.nodes }
}

const NUMA_TOTAL: &str = "total";

impl HugetlbNumaStat {
  /// Encodes the stored value as canonical cgroup text.
  ///
  /// # Errors
  ///
  /// Returns an error if a field cannot be represented exactly.
  pub fn encode_cgroup(&self) -> Result<String, crate::error::EncodeError> {
    let mut output = String::new();
    crate::parse::encode_pair::<ParseBytes, _>(&mut output, " ", '=', NUMA_TOTAL, &self.total)?;
    for (node, value) in &self.nodes {
      write!(output, " N{node}=").map_err(|_error| crate::error::EncodeError { field: "node" })?;
      EncodeCgroup::<ParseBytes>::encode_field(value, &mut output, "usage")?;
    }
    Ok(output)
  }
}

impl FromStr for HugetlbNumaStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::new(
      contents,
      contents.split_ascii_whitespace().map(|token| {
        token.split_once('=').ok_or_else(|| ParseError::invalid(contents, "value", token, ParseValueError::OutOfRange))
      }),
    )?;
    let total = fields.required::<ParseBytes, _>(NUMA_TOTAL)?;
    let nodes = fields
      .iter()
      .filter_map(|(key, value)| key.strip_prefix('N').map(|node| (key, node, value)))
      .map(|(key, node, value)| {
        let node = node.parse::<u32>().map_err(|error| ParseError::invalid(contents, "node", key, error.into()))?;
        Ok((node, Bytes::parse_field(contents, "usage", value)?))
      })
      .collect::<Result<_, Self::Err>>()?;
    Ok(Self { total, nodes })
  }
}

crate::v2::interface::cgroup_file!(file HugetlbCurrent, HugetlbCurrent::FILE_NAME_TEMPLATE, Read,
  PageSize => |size: &PageSize| size.file_name(HugetlbCurrent::SUFFIX));
crate::v2::interface::cgroup_file!(read HugetlbCurrent => HugetlbCurrent);

crate::v2::interface::cgroup_file!(file HugetlbMax, HugetlbMax::FILE_NAME_TEMPLATE, ReadWrite,
  PageSize => |size: &PageSize| size.file_name(HugetlbMax::SUFFIX));
crate::v2::interface::cgroup_file!(read HugetlbMax => HugetlbMax);

crate::v2::interface::cgroup_file!(write HugetlbMax, 'a => MaxOr<Bytes>, |value| HugetlbMax { value }.encode_cgroup());

crate::v2::interface::cgroup_file!(file HugetlbReservedCurrent, HugetlbReservedCurrent::FILE_NAME_TEMPLATE, Read,
  PageSize => |size: &PageSize| size.file_name(HugetlbReservedCurrent::SUFFIX));
crate::v2::interface::cgroup_file!(read HugetlbReservedCurrent => HugetlbReservedCurrent);

crate::v2::interface::cgroup_file!(file HugetlbReservedMax, HugetlbReservedMax::FILE_NAME_TEMPLATE, ReadWrite,
  PageSize => |size: &PageSize| size.file_name(HugetlbReservedMax::SUFFIX));
crate::v2::interface::cgroup_file!(read HugetlbReservedMax => HugetlbReservedMax);

crate::v2::interface::cgroup_file!(write HugetlbReservedMax, 'a => MaxOr<Bytes>, |value| HugetlbReservedMax { value }.encode_cgroup());

crate::v2::interface::cgroup_file!(file HugetlbEvents, HugetlbEvents::FILE_NAME_TEMPLATE, Read,
  PageSize => |size: &PageSize| size.file_name(HugetlbEvents::SUFFIX));
crate::v2::interface::cgroup_file!(read HugetlbEvents => HugetlbEvents);

crate::v2::interface::cgroup_file!(file HugetlbNumaStat, HugetlbNumaStat::FILE_NAME_TEMPLATE, Read,
  PageSize => |size: &PageSize| size.file_name(HugetlbNumaStat::SUFFIX));
crate::v2::interface::cgroup_file!(read HugetlbNumaStat => HugetlbNumaStat);

crate::v2::interface::cgroup_file!(file HugetlbEventsLocal, HugetlbEvents::LOCAL_FILE_NAME_TEMPLATE, Read,
  PageSize => |size: &PageSize| size.file_name(HugetlbEvents::LOCAL_SUFFIX));
crate::v2::interface::cgroup_file!(read HugetlbEventsLocal => HugetlbEvents);

/// Local HugeTLB allocation failures for a given page size.
pub struct HugetlbEventsLocal;

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };
  #[cfg(target_os = "linux")]
  use crate::{Cgroup, Error};

  #[test]
  fn validates_page_sizes_and_canonical_kernel_labels() {
    for (label, bytes) in
      [("1KB", 1024), ("64KB", 65536), ("2MB", 2097152), ("1GB", 1073741824), ("8192GB", 8796093022208)]
    {
      let size = assert_ok!(label.parse::<PageSize>());
      assert_eq!(size, assert_ok!(PageSize::try_new(Bytes::new::<byte>(bytes))));
      assert_eq!(size.to_string(), label);
      assert_eq!(size.into_inner().get::<byte>(), bytes);
      assert_eq!(size.file_name("current"), format!("hugetlb.{label}.current"));
    }
    for label in [
      "",
      "0KB",
      "3MB",
      "512",
      "2mb",
      "02MB",
      "+2MB",
      "2048KB",
      "1024MB",
      "../2MB",
      "2MB/../",
      "2 MB",
      "2MB\n",
      "18446744073709551615GB",
    ] {
      assert_err!(label.parse::<PageSize>());
    }
    for bytes in [0, 512, 1023, 1025, u64::MAX] {
      assert_err!(PageSize::try_new(Bytes::new::<byte>(bytes)));
    }
  }

  #[test]
  fn parses_byte_usage_and_limits_without_narrowing() {
    for value in [0, 1, 4194304, u64::MAX] {
      let input = format!("{value}\n");
      assert_eq!(assert_ok!(input.parse::<HugetlbCurrent>()), HugetlbCurrent { value: Bytes::new::<byte>(value) });
      assert_eq!(assert_ok!(input.parse::<HugetlbReservedCurrent>()), HugetlbReservedCurrent {
        value: Bytes::new::<byte>(value)
      });
      assert_eq!(assert_ok!(input.parse::<HugetlbMax>()), HugetlbMax {
        value: MaxOr::Value(Bytes::new::<byte>(value))
      });
      assert_eq!(assert_ok!(input.parse::<HugetlbReservedMax>()), HugetlbReservedMax {
        value: MaxOr::Value(Bytes::new::<byte>(value))
      });
    }
    assert_eq!(assert_ok!("max".parse::<HugetlbMax>()), HugetlbMax { value: MaxOr::Max });
    assert_eq!(assert_ok!("max".parse::<HugetlbReservedMax>()), HugetlbReservedMax { value: MaxOr::Max });
    Cases::<HugetlbCurrent>::check([
      ("", Err(Missing("current"))),
      ("max", Err(Invalid("current", "max"))),
      ("-1", Err(Invalid("current", "-1"))),
      ("18446744073709551616", Err(Invalid("current", "18446744073709551616"))),
      ("1 2", Err(Excess)),
    ]);
    Cases::<HugetlbMax>::check([
      ("", Err(Missing("max"))),
      ("nope", Err(Invalid("max", "nope"))),
      ("-1", Err(Invalid("max", "-1"))),
      ("18446744073709551616", Err(Invalid("max", "18446744073709551616"))),
      ("max 2", Err(Excess)),
    ]);
  }

  #[test]
  fn parses_keyed_events_and_numa_bytes() {
    Cases::<HugetlbEvents>::check([
      (
        include_str!("fixtures/p6_hugetlb/hugetlb.2MB.events"),
        Ok(HugetlbEvents { max: Count { value: 3, ..Default::default() } }),
      ),
      (include_str!("fixtures/p6_hugetlb_older/hugetlb.2MB.events"), Ok(HugetlbEvents { max: Count::default() })),
      ("max 18446744073709551615", Ok(HugetlbEvents { max: Count { value: u64::MAX, ..Default::default() } })),
      ("", Err(Missing("max"))),
      ("max", Err(Missing("value"))),
      ("max -1", Err(Invalid("max", "-1"))),
      ("max nope", Err(Invalid("max", "nope"))),
      ("max 1 extra", Err(Excess)),
    ]);
    Cases::<HugetlbNumaStat>::check([
      (
        include_str!("fixtures/p6_hugetlb/hugetlb.2MB.numa_stat"),
        Ok(HugetlbNumaStat {
          total: Bytes::new::<byte>(4194304),
          nodes: BTreeMap::from([(0, Bytes::new::<byte>(2097152)), (3, Bytes::new::<byte>(2097152))]),
        }),
      ),
      (
        "total=18446744073709551615 N0=0 N4294967295=18446744073709551615",
        Ok(HugetlbNumaStat {
          total: Bytes::new::<byte>(u64::MAX),
          nodes: BTreeMap::from([(0, Bytes::new::<byte>(0)), (u32::MAX, Bytes::new::<byte>(u64::MAX))]),
        }),
      ),
      ("total=0", Ok(HugetlbNumaStat { total: Bytes::new::<byte>(0), nodes: BTreeMap::new() })),
      ("", Err(Missing("total"))),
      ("N0=0", Err(Missing("total"))),
      ("total=nope", Err(Invalid("total", "nope"))),
      ("total=0 Nbad=1", Err(Invalid("node", "Nbad"))),
      ("total=0 N4294967296=1", Err(Invalid("node", "N4294967296"))),
      ("total=0 N0=-1", Err(Invalid("usage", "-1"))),
      ("total=0 N0", Err(Invalid("value", "N0"))),
    ]);
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn reads_fixtures_and_reports_missing_optional_and_root_files() {
    let size = assert_ok!("2MB".parse::<PageSize>());
    for (fixture, current, max, reserved_current, events, local_events) in [
      ("p6_hugetlb", 4194304, MaxOr::Value(Bytes::new::<byte>(8388608)), 6291456, 3, 1),
      ("p6_hugetlb_older", 0, MaxOr::Max, 0, 0, 0),
    ] {
      let cgroup = Cgroup::fixture(&format!("fixtures/{fixture}"));
      assert_eq!(assert_ok!(cgroup.read_file::<HugetlbCurrent>(&size)), HugetlbCurrent {
        value: Bytes::new::<byte>(current)
      });
      assert_eq!(assert_ok!(cgroup.read_file::<HugetlbMax>(&size)), HugetlbMax { value: max });
      assert_eq!(assert_ok!(cgroup.read_file::<HugetlbReservedCurrent>(&size)), HugetlbReservedCurrent {
        value: Bytes::new::<byte>(reserved_current)
      });
      assert_eq!(assert_ok!(cgroup.read_file::<HugetlbReservedMax>(&size)), HugetlbReservedMax { value: MaxOr::Max });
      assert_eq!(assert_ok!(cgroup.read_file::<HugetlbEvents>(&size)), HugetlbEvents {
        max: Count { value: events, ..Default::default() }
      });
      assert_eq!(assert_ok!(cgroup.read_file::<HugetlbEventsLocal>(&size)), HugetlbEvents {
        max: Count { value: local_events, ..Default::default() }
      });
      match fixture {
        | "p6_hugetlb" => {
          cgroup.check_fixture(&size.file_name("numa_stat"), false, || cgroup.read_file::<HugetlbNumaStat>(&size))
        },
        | _ => assert!(
          matches!(cgroup.read_file::<HugetlbNumaStat>(&size), Err(Error::FileMissing { path }) if path == cgroup.path().join(size.file_name("numa_stat")))
        ),
      }
    }
    let root = Cgroup::fixture("");
    for (suffix, result) in [
      ("current", root.read_file::<HugetlbCurrent>(&size).map(|_| ())),
      ("max", root.read_file::<HugetlbMax>(&size).map(|_| ())),
      ("rsvd.current", root.read_file::<HugetlbReservedCurrent>(&size).map(|_| ())),
      ("rsvd.max", root.read_file::<HugetlbReservedMax>(&size).map(|_| ())),
      ("events", root.read_file::<HugetlbEvents>(&size).map(|_| ())),
      ("events.local", root.read_file::<HugetlbEventsLocal>(&size).map(|_| ())),
      ("numa_stat", root.read_file::<HugetlbNumaStat>(&size).map(|_| ())),
    ] {
      assert!(matches!(result, Err(Error::FileMissing { path }) if path == root.path().join(size.file_name(suffix))));
    }
  }
}
