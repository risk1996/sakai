//! Device-memory accounting, capacity, and per-region configuration snapshots.
//!
//! Region names are opaque driver-defined identifiers, not paths to open.
//! Every value is measured in bytes, independently of device or system page size.
//! Since Linux 6.14; see the [versioned Linux interfaces](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).

use std::{collections::BTreeMap, str::FromStr};

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::Error;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{KeyedFields, ParseBytes, ParseCgroup},
  unit::Bytes,
};

/// A borrowed view of one cgroup's device-memory controller.
///
/// Each method performs fresh read-only I/O; separate reads are not atomic.
/// Since Linux 6.14 and requires `CONFIG_CGROUP_DMEM` and controller availability.
/// Missing files return [`crate::Error::FileMissing`], including older kernels
/// and root exemptions. Capacity is root-only; protection and limit files are
/// non-root. The kernel also exposes current usage at the root, although the
/// interface guide describes it as non-root.
///
/// See [Linux 6.14 DMEM source](https://github.com/torvalds/linux/blob/v6.14/kernel/cgroup/dmem.c).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Dmem<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Dmem<'_> {
  /// Reads registered region capacities in bytes from the root-only interface.
  ///
  /// Capacity includes device memory reserved for kernel use, so it is not
  /// necessarily available to cgroups. Since Linux 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn capacity(&self) -> Result<DmemCapacity, Error> { self.cgroup.parse(DmemCapacity::FILE_NAME) }

  /// Reads volatile hierarchical usage for each registered region in bytes.
  ///
  /// Since Linux 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn current(&self) -> Result<DmemCurrent, Error> { self.cgroup.parse(DmemCurrent::FILE_NAME) }

  /// Reads historical hierarchical device-memory usage peaks per region.
  ///
  /// Since Linux 7.3 release candidates; non-root only. Each region has an
  /// independent lifetime peak. Reading never resets peaks. See [`DmemPeak`].
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn peak(&self) -> Result<DmemPeak, Error> { self.cgroup.parse(DmemPeak::FILE_NAME) }

  /// Reads configured hard eviction protections in bytes, or `max`.
  ///
  /// Effective protection depends on ancestors and sibling demand. Since Linux
  /// 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn min(&self) -> Result<DmemLimits, Error> { self.cgroup.parse(DmemLimits::MIN_FILE_NAME) }

  /// Reads configured best-effort eviction protections in bytes, or `max`.
  ///
  /// Effective protection depends on ancestors and sibling demand. Since Linux
  /// 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn low(&self) -> Result<DmemLimits, Error> { self.cgroup.parse(DmemLimits::LOW_FILE_NAME) }

  /// Reads configured hierarchical usage limits in bytes, or `max`.
  ///
  /// `max` removes this cgroup's limit; ancestor limits still apply. Since Linux
  /// 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max(&self) -> Result<DmemLimits, Error> { self.cgroup.parse(DmemLimits::MAX_FILE_NAME) }
}

/// A snapshot of registered device-memory region capacities in bytes.
///
/// The root-only `dmem.capacity` interface includes memory reserved for kernel
/// use; these amounts are not guarantees of allocatable cgroup memory. An empty
/// map means no regions were reported. Region membership can change between
/// reads; arbitrary driver-defined names are retained without interpretation.
/// Getters inspect this stored snapshot without I/O.
///
/// Since Linux 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DmemCapacity {
  regions: BTreeMap<String, Bytes>,
}

impl DmemCapacity {
  /// The root-only cgroup v2 capacity interface filename, since Linux 6.14.
  pub const FILE_NAME: &'static str = "dmem.capacity";

  /// Returns capacities in bytes keyed by opaque region name.
  #[must_use]
  pub const fn regions(&self) -> &BTreeMap<String, Bytes> { &self.regions }
}

impl FromStr for DmemCapacity {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> { Ok(Self { regions: parse_regions(contents)? }) }
}

/// A volatile snapshot of device-memory usage for this cgroup and descendants.
///
/// Amounts are bytes, not counts of device or system pages. Zero means a
/// reported region has no usage; an absent region means it was not reported.
/// The kernel emits `max` when usage reaches its `PAGE_COUNTER_MAX` sentinel;
/// that token is retained as [`MaxOr::Max`], rather than interpreted as an
/// unlimited configured limit. The sentinel depends on the kernel architecture
/// and system page size. See [Linux output formatting](https://github.com/torvalds/linux/blob/v6.14/kernel/cgroup/dmem.c).
/// An empty map is valid. Region names and usage can change between reads;
/// getters inspect this stored snapshot without I/O. Separately read capacity
/// and configuration values need not describe the same instant.
///
/// Since Linux 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DmemCurrent {
  regions: BTreeMap<String, MaxOr<Bytes>>,
}

impl DmemCurrent {
  /// The cgroup v2 current device-memory usage filename, since Linux 6.14.
  pub const FILE_NAME: &'static str = "dmem.current";

  /// Returns byte usage or the kernel's `max` sentinel by opaque region name.
  #[must_use]
  pub const fn regions(&self) -> &BTreeMap<String, MaxOr<Bytes>> { &self.regions }
}

impl FromStr for DmemCurrent {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> { Ok(Self { regions: parse_regions(contents)? }) }
}

/// Historical maximum device-memory usage by region for this cgroup and descendants.
///
/// Each finite value is bytes. `max` retains the kernel's architecture-dependent
/// `PAGE_COUNTER_MAX` sentinel, as in [`DmemCurrent`]. Independent region peaks
/// include transient charges later canceled by ancestor-limit failures. Region
/// removal removes its history. Empty maps are valid; absent regions differ
/// from measured zero. Getters inspect this stored snapshot without I/O.
/// Reading never resets peaks; the hierarchy root lacks this interface.
///
/// Since Linux 7.3 release candidates; see [Linux 7.3-rc1 source](https://github.com/torvalds/linux/blob/v7.3-rc1/kernel/cgroup/dmem.c)
/// and [Linux `dmem.peak`](https://docs.kernel.org/admin-guide/cgroup-v2.html#dmem).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DmemPeak {
  regions: BTreeMap<String, MaxOr<Bytes>>,
}

impl DmemPeak {
  /// The non-root historical device-memory peak filename, since Linux 7.3 release candidates.
  pub const FILE_NAME: &'static str = "dmem.peak";

  /// Returns historical byte peaks or the kernel's `max` sentinel by region name.
  #[must_use]
  pub const fn regions(&self) -> &BTreeMap<String, MaxOr<Bytes>> { &self.regions }
}

impl FromStr for DmemPeak {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> { Ok(Self { regions: parse_regions(contents)? }) }
}

/// A configuration snapshot from `dmem.max`, `dmem.min`, or `dmem.low`.
///
/// Amounts are bytes; all three interfaces can report `max`. For `dmem.max`,
/// it means no limit at this cgroup, while ancestor limits still apply. For
/// `dmem.min` and `dmem.low`, it requests protection without a numeric upper
/// boundary. The effective hard (`min`) or best-effort (`low`) eviction
/// protection depends on ancestor protection and sibling demand; these are
/// configured values, not calculated effective protections. Zero is valid.
/// The files are absent at the root. An empty map is valid; region names and
/// configuration may change between reads. Getters inspect stored values.
///
/// Since Linux 6.14; see [Linux DMEM](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html#dmem)
/// and the [versioned implementation](https://github.com/torvalds/linux/blob/v6.14/kernel/cgroup/dmem.c).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DmemLimits {
  regions: BTreeMap<String, MaxOr<Bytes>>,
}

impl DmemLimits {
  /// The non-root best-effort eviction protection filename, since Linux 6.14.
  pub const LOW_FILE_NAME: &'static str = "dmem.low";
  /// The non-root hard device-memory limit filename, since Linux 6.14.
  pub const MAX_FILE_NAME: &'static str = "dmem.max";
  /// The non-root hard eviction protection filename, since Linux 6.14.
  pub const MIN_FILE_NAME: &'static str = "dmem.min";

  /// Returns configured byte amounts or `max`, keyed by opaque region name.
  #[must_use]
  pub const fn regions(&self) -> &BTreeMap<String, MaxOr<Bytes>> { &self.regions }
}

impl FromStr for DmemLimits {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> { Ok(Self { regions: parse_regions(contents)? }) }
}

fn parse_regions<T>(contents: &str) -> Result<BTreeMap<String, T>, ParseError<ParseValueError>>
where
  T: ParseCgroup<ParseBytes, Error = ParseValueError>, {
  KeyedFields::parse(contents, |_| "bytes")?.parse_values::<ParseBytes, T>("bytes")
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use uom::si::information::byte;

  use super::*;
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
      let reader = Dmem { cgroup: &cgroup };
      cgroup.check_fixture(DmemCapacity::FILE_NAME, true, || reader.capacity());
      cgroup.check_fixture(DmemCurrent::FILE_NAME, true, || reader.current());
      cgroup.check_fixture(DmemPeak::FILE_NAME, true, || reader.peak());
      cgroup.check_fixture(DmemLimits::MIN_FILE_NAME, true, || reader.min());
      cgroup.check_fixture(DmemLimits::LOW_FILE_NAME, true, || reader.low());
      cgroup.check_fixture(DmemLimits::MAX_FILE_NAME, true, || reader.max());
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
