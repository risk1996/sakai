//! Device-memory accounting, capacity, and per-region configuration snapshots.
//!
//! Region names are opaque driver-defined identifiers, not paths to open.
//! Every value is measured in bytes, independently of device or system page size.
//! Since Linux 6.14; see the [versioned Linux interfaces](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).

use std::collections::BTreeMap;

use crate::{
  limit::MaxOr,
  parse::{ParseBytes, cgroup_value},
  unit::Bytes,
};

/// A snapshot of registered device-memory region capacities in bytes.
///
/// The root-only `dmem.capacity` interface includes memory reserved for kernel
/// use; these amounts are not guarantees of allocatable cgroup memory. An empty
/// map means no regions were reported. Region membership can change between
/// reads; arbitrary driver-defined names are retained without interpretation.
///
/// Since Linux 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct DmemCapacity {
  /// Capacities in bytes keyed by opaque region name.
  pub regions: BTreeMap<String, Bytes>,
}

crate::v2::interface::cgroup_file!(DmemCapacity, "dmem.capacity", Read);

impl DmemCapacity {
  /// The root-only cgroup v2 capacity interface filename, since Linux 6.14.
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;
}

cgroup_value! { map DmemCapacity, regions: ParseBytes, "bytes" }

/// A volatile snapshot of device-memory usage for this cgroup and descendants.
///
/// Amounts are bytes, not counts of device or system pages. Zero means a
/// reported region has no usage; an absent region means it was not reported.
/// The kernel emits `max` when usage reaches its `PAGE_COUNTER_MAX` sentinel;
/// that token is retained as [`MaxOr::Max`], rather than interpreted as an
/// unlimited configured limit. The sentinel depends on the kernel architecture
/// and system page size. See [Linux output formatting](https://github.com/torvalds/linux/blob/v6.14/kernel/cgroup/dmem.c).
/// An empty map is valid. Region names and usage can change between reads.
/// Separately read usage, capacity, and configuration may describe different instants.
///
/// Since Linux 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct DmemCurrent {
  /// Hierarchical usage in bytes or the kernel's `max` sentinel by region.
  pub regions: BTreeMap<String, MaxOr<Bytes>>,
}

crate::v2::interface::cgroup_file!(DmemCurrent, "dmem.current", Read);

impl DmemCurrent {
  /// The cgroup v2 current device-memory usage filename, since Linux 6.14.
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;
}

cgroup_value! { map DmemCurrent, regions: ParseBytes, "bytes" }

/// Historical maximum device-memory usage by region for this cgroup and descendants.
///
/// Each finite value is bytes. `max` retains the kernel's architecture-dependent
/// `PAGE_COUNTER_MAX` sentinel, as in [`DmemCurrent`]. Independent region peaks
/// include transient charges later canceled by ancestor-limit failures. Region
/// removal removes its history. Empty maps are valid; absent regions differ
/// from measured zero.
/// Reading never resets peaks; the hierarchy root lacks this interface.
///
/// Since Linux 7.3; see [Linux 7.3 source](https://github.com/torvalds/linux/blob/v7.3/kernel/cgroup/dmem.c)
/// and [Linux `dmem.peak`](https://docs.kernel.org/admin-guide/cgroup-v2.html#dmem).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct DmemPeak {
  /// Historical byte peaks or the kernel's `max` sentinel by region name.
  pub regions: BTreeMap<String, MaxOr<Bytes>>,
}

crate::v2::interface::cgroup_file!(DmemPeak, "dmem.peak", Read);

impl DmemPeak {
  /// The non-root historical device-memory peak filename, since Linux 7.3.
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;
}

cgroup_value! { map DmemPeak, regions: ParseBytes, "bytes" }

/// A configuration snapshot from `dmem.max`, `dmem.min`, or `dmem.low`.
///
/// Amounts are bytes; all three interfaces can report `max`. For `dmem.max`,
/// it means no limit at this cgroup, while ancestor limits still apply. For
/// `dmem.min` and `dmem.low`, it requests protection without a numeric upper
/// boundary. The effective hard (`min`) or best-effort (`low`) eviction
/// protection depends on ancestor protection and sibling demand; these are
/// configured values, not calculated effective protections. Zero is valid.
/// The files are absent at the root. An empty map is valid; region names and
/// configuration may change between reads.
///
/// Since Linux 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem)
/// and the [versioned implementation](https://github.com/torvalds/linux/blob/v6.14/kernel/cgroup/dmem.c).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct DmemLimits {
  /// Configured byte amounts or `max`, keyed by opaque region name.
  pub regions: BTreeMap<String, MaxOr<Bytes>>,
}

impl DmemLimits {
  /// The non-root best-effort eviction protection filename, since Linux 6.14.
  pub const LOW_FILE_NAME: &'static str = "dmem.low";
  /// The non-root hard device-memory limit filename, since Linux 6.14.
  pub const MAX_FILE_NAME: &'static str = "dmem.max";
  /// The non-root hard eviction protection filename, since Linux 6.14.
  pub const MIN_FILE_NAME: &'static str = "dmem.min";
}

cgroup_value! { map DmemLimits, regions: ParseBytes, "bytes" }

crate::v2::interface::cgroup_file!(alias
  /// Device-memory min by region.
  DmemMin, DmemLimits::MIN_FILE_NAME, ReadWrite, DmemLimits
);

crate::v2::interface::cgroup_file!(write DmemMin, 'a => (&'a str, MaxOr<Bytes>), encode_region);

crate::v2::interface::cgroup_file!(alias
  /// Device-memory low by region.
  DmemLow, DmemLimits::LOW_FILE_NAME, ReadWrite, DmemLimits
);

crate::v2::interface::cgroup_file!(write DmemLow, 'a => (&'a str, MaxOr<Bytes>), encode_region);

crate::v2::interface::cgroup_file!(alias
  /// Device-memory max by region.
  DmemMax, DmemLimits::MAX_FILE_NAME, ReadWrite, DmemLimits
);

crate::v2::interface::cgroup_file!(write DmemMax, 'a => (&'a str, MaxOr<Bytes>), encode_region);

fn encode_region((region, value): (&str, MaxOr<Bytes>)) -> Result<String, crate::error::EncodeError> {
  super::interface::encode_keyed::<ParseBytes>(region, "region", &value, "bytes")
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use uom::si::information::byte;

  use super::*;
  #[cfg(target_os = "linux")]
  use crate::Cgroup;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

  fn regions(values: &[(&str, u64)]) -> BTreeMap<String, Bytes> {
    values.iter().map(|(name, value)| ((*name).into(), Bytes::new::<byte>(*value))).collect()
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn reads_region_fixtures_without_writes_and_preserves_missing_paths() {
    for fixture in ["p6_dmem", "p6_dmem_root", "p6_dmem_older"] {
      let cgroup = Cgroup::fixture(&format!("fixtures/{fixture}"));
      cgroup.check_fixture(DmemCapacity::FILE_NAME, true, || cgroup.read_value::<DmemCapacity>());
      cgroup.check_fixture(DmemCurrent::FILE_NAME, true, || cgroup.read_value::<DmemCurrent>());
      cgroup.check_fixture(DmemPeak::FILE_NAME, true, || cgroup.read_value::<DmemPeak>());
      cgroup.check_fixture(DmemLimits::MIN_FILE_NAME, true, || cgroup.read_value::<DmemMin>());
      cgroup.check_fixture(DmemLimits::LOW_FILE_NAME, true, || cgroup.read_value::<DmemLow>());
      cgroup.check_fixture(DmemLimits::MAX_FILE_NAME, true, || cgroup.read_value::<DmemMax>());
    }
  }

  #[test]
  fn parses_region_capacity_and_usage_in_bytes() {
    for (input, expected) in [
      (
        include_str!("fixtures/p6_dmem_root/dmem.capacity"),
        regions(&[("drm/0000:03:00.0/vram0", 8_514_437_120), ("drm/0000:03:00.0/stolen", 67_108_864)]),
      ),
      (
        include_str!("fixtures/p6_dmem/dmem.current"),
        regions(&[("drm/0000:03:00.0/vram0", 12_550_144), ("drm/0000:03:00.0/stolen", 0), ("future/device", u64::MAX)]),
      ),
      (" \n", BTreeMap::new()),
    ] {
      assert_eq!(assert_ok!(input.parse::<DmemCapacity>()), DmemCapacity { regions: expected.clone() });
      let expected = expected.into_iter().map(|(name, value)| (name, MaxOr::Value(value))).collect();
      assert_eq!(assert_ok!(input.parse::<DmemCurrent>()), DmemCurrent { regions: expected });
    }
    assert_err!("device max".parse::<DmemCapacity>());
    for input in ["device", "device -1", "device nope", "device 18446744073709551616", "device 1 extra"] {
      assert_err!(input.parse::<DmemCapacity>());
      assert_err!(input.parse::<DmemCurrent>());
      assert_err!(input.parse::<DmemPeak>());
    }
  }

  #[test]
  fn preserves_current_and_peak_kernel_sentinels() {
    let expected = BTreeMap::from([
      ("device".into(), MaxOr::Max),
      ("zero".into(), MaxOr::Value(Bytes::new::<byte>(0))),
      ("finite".into(), MaxOr::Value(Bytes::new::<byte>(123))),
    ]);
    let raw = "device max\nzero 0\nfinite 123";
    assert_eq!(assert_ok!(raw.parse::<DmemCurrent>()), DmemCurrent { regions: expected.clone() });
    assert_eq!(assert_ok!(raw.parse::<DmemPeak>()), DmemPeak { regions: expected });
    assert_eq!(assert_ok!("".parse::<DmemPeak>()), DmemPeak::default());
  }

  #[test]
  fn parses_limits_and_protections_with_max() {
    for name in ["max", "min", "low"] {
      let input = match name {
        | "max" => include_str!("fixtures/p6_dmem/dmem.max"),
        | "min" => include_str!("fixtures/p6_dmem/dmem.min"),
        | _ => include_str!("fixtures/p6_dmem/dmem.low"),
      };
      assert_eq!(assert_ok!(input.parse::<DmemLimits>()), DmemLimits {
        regions: BTreeMap::from([
          ("drm/0000:03:00.0/vram0".into(), MaxOr::Value(Bytes::new::<byte>(1_073_741_824))),
          ("drm/0000:03:00.0/stolen".into(), MaxOr::Max),
          ("future/device".into(), MaxOr::Value(Bytes::new::<byte>(0))),
        ]),
      });
    }
    Cases::<DmemLimits>::check([
      ("", Ok(DmemLimits::default())),
      ("device", Err(Missing("bytes"))),
      ("device -1", Err(Invalid("bytes", "-1"))),
      ("device nope", Err(Invalid("bytes", "nope"))),
      ("device 18446744073709551616", Err(Invalid("bytes", "18446744073709551616"))),
      ("device 1 extra", Err(Excess)),
    ]);
  }
}
