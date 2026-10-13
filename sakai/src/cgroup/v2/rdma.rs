//! Per-device RDMA resource accounting, limits, and allocation events.
//!
//! Device names identify RDMA/InfiniBand adapters rather than block devices.
//! Resources remain charged to their owning cgroup after task migration.

use std::{collections::BTreeMap, str::FromStr};

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::Error;
use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{KeyedFields, ParseCount},
  unit::Count,
};

/// An RDMA adapter name and its optional system-wide device index.
///
/// Names can be shared by adapters in different network namespaces. Since
/// Linux 7.3 release candidates, the kernel adds `index=N` when a name is
/// ambiguous; see the [Linux 7.3-rc1 implementation](https://github.com/torvalds/linux/blob/v7.3-rc1/kernel/cgroup/rdma.c).
/// Older kernels and globally unique names omit the index. `None` means
/// unreported, not index zero. Indices are unsigned identifiers, not counts.
///
/// This key describes one stored record, not a permanent device identity.
/// Renames, registration changes, and index reuse can change identity between
/// snapshots. An index can appear or disappear when another adapter shares
/// the name. See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RdmaDevice {
  name: String,
  index: Option<u32>,
}

impl RdmaDevice {
  /// Constructs a record key without resolving a currently registered adapter.
  ///
  /// Preserve `None` when the kernel omitted the index; it differs from zero.
  #[must_use]
  pub fn new(name: impl Into<String>, index: Option<u32>) -> Self { Self { name: name.into(), index } }

  /// Returns the stored adapter name, which can be shared across namespaces.
  #[must_use]
  pub fn name(&self) -> &str { &self.name }

  /// Returns the reported system-wide device index, or `None` when omitted.
  ///
  /// Inspects this record without performing I/O or resolving a device.
  #[must_use]
  pub const fn index(&self) -> Option<u32> { self.index }
}

/// A borrowed view of one cgroup's RDMA controller.
///
/// Each call reads a fresh, read-only snapshot. Separate calls are not atomic
/// together. Interfaces are absent on the hierarchy root and when the RDMA
/// controller is disabled or unavailable in the kernel configuration.
/// See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Rdma<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Rdma<'_> {
  /// Reads hierarchical HCA handle and object usage, measured in counts.
  ///
  /// Returns a fresh [`RdmaCurrent`] snapshot keyed by [`RdmaDevice`]. See
  /// [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn current(&self) -> Result<RdmaCurrent, Error> { self.cgroup.parse(RdmaCurrent::FILE_NAME) }

  /// Reads per-device HCA handle and object limits, measured in counts.
  ///
  /// Returns a fresh [`RdmaMax`] configuration snapshot. `max` means no
  /// configured limit here; ancestor limits still apply. See
  /// [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max(&self) -> Result<RdmaMax, Error> { self.cgroup.parse(RdmaMax::FILE_NAME) }

  /// Reads historical hierarchical HCA resource high-water marks in counts.
  ///
  /// Since Linux 7.2; see the
  /// [release-tagged implementation](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
  /// Each resource's peak is independent; the two peaks need not have occurred
  /// together. Returns a fresh [`RdmaCurrent`] snapshot without resetting peaks.
  ///
  /// # Errors
  ///
  /// Older kernels return [`Error::FileMissing`]. See the shared
  /// [interface-read errors](crate::v2#interface-read-errors) for other failures.
  pub fn peak(&self) -> Result<RdmaCurrent, Error> { self.cgroup.parse(RdmaCurrent::PEAK_FILE_NAME) }

  /// Reads hierarchical per-device RDMA allocation-failure event counts.
  ///
  /// Since Linux 7.2; see the
  /// [release-tagged implementation](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
  /// Returns a fresh [`RdmaEvents`] snapshot including descendants' events.
  ///
  /// # Errors
  ///
  /// Older kernels return [`Error::FileMissing`]. See the shared
  /// [interface-read errors](crate::v2#interface-read-errors) for other failures.
  pub fn events(&self) -> Result<RdmaEvents, Error> { self.cgroup.parse(RdmaEvents::FILE_NAME) }

  /// Reads local per-device RDMA allocation-failure event counts.
  ///
  /// Since Linux 7.2; see the
  /// [release-tagged implementation](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
  /// Returns a fresh [`RdmaEvents`] snapshot. Local `max` counts failures
  /// against this cgroup's own limit, including descendant requests; local
  /// `alloc_fail` counts requests originating here, including ancestor limits.
  ///
  /// # Errors
  ///
  /// Older kernels return [`Error::FileMissing`]. See the shared
  /// [interface-read errors](crate::v2#interface-read-errors) for other failures.
  pub fn events_local(&self) -> Result<RdmaEvents, Error> { self.cgroup.parse(RdmaEvents::LOCAL_FILE_NAME) }
}

/// A volatile snapshot of hierarchical RDMA resource usage or peak usage.
///
/// Counts include this cgroup and its descendants, keyed by [`RdmaDevice`]
/// name/index pairs. An empty map means no devices were reported. Current usage
/// can rise
/// or fall between reads; peaks are independent historical high-water marks
/// since cgroup creation. Device removal also removes its reported history.
/// Optional device indices are preserved in the map keys.
/// Unknown resource keys are ignored; the two baseline counters are required.
/// See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RdmaCurrent {
  devices: BTreeMap<RdmaDevice, RdmaDeviceCurrent>,
}

impl RdmaCurrent {
  /// The cgroup v2 `rdma.current` interface filename.
  pub const FILE_NAME: &'static str = "rdma.current";
  /// The `rdma.peak` interface filename, available since Linux 7.2.
  ///
  /// See the [Linux 7.2 implementation](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
  pub const PEAK_FILE_NAME: &'static str = "rdma.peak";

  /// Returns stored per-device HCA counts without performing I/O.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<RdmaDevice, RdmaDeviceCurrent> { &self.devices }
}

/// Hierarchical HCA handle and object counts for one RDMA device.
///
/// HCA handles count userspace adapter contexts; HCA objects count resources
/// such as queue pairs, memory regions, and completion queues. These are
/// counts, not byte quantities. The kernel emits `max` when a usage or peak
/// count reaches its signed 32-bit maximum; this reader preserves that exact
/// count as 2,147,483,647 rather than treating usage as unlimited. See the
/// [kernel read implementation](https://github.com/torvalds/linux/blob/v5.15/kernel/cgroup/rdma.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RdmaDeviceCurrent {
  hca_handle: Count,
  hca_object: Count,
}

impl RdmaDeviceCurrent {
  /// Returns the stored number of HCA handles.
  #[must_use]
  pub const fn hca_handle(&self) -> Count { self.hca_handle }

  /// Returns the stored number of HCA objects.
  #[must_use]
  pub const fn hca_object(&self) -> Count { self.hca_object }

  fn from_fields(fields: KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    let count = |field| {
      fields.required::<ParseCount, MaxOr<Count>>(field).map(|value| match value {
        | MaxOr::Value(count) => count,
        | MaxOr::Max => Count { value: i32::MAX as u64, ..Default::default() },
      })
    };
    Ok(Self { hca_handle: count("hca_handle")?, hca_object: count("hca_object")? })
  }
}

impl FromStr for RdmaCurrent {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self { devices: parse_devices(contents, RdmaDeviceCurrent::from_fields)? })
  }
}

/// A per-device configuration snapshot from `rdma.max`, measured in counts.
///
/// Zero is a valid limit. `max` means no configured limit at this cgroup;
/// ancestor limits still apply. Configuration and registered devices can
/// change between reads. An empty map means no devices were reported.
/// Optional device indices are preserved in the map keys.
/// Unknown resource keys are ignored; the two baseline limits are required.
/// See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RdmaMax {
  devices: BTreeMap<RdmaDevice, RdmaDeviceMax>,
}

impl RdmaMax {
  /// The cgroup v2 `rdma.max` interface filename.
  pub const FILE_NAME: &'static str = "rdma.max";

  /// Returns stored per-device limits without performing I/O.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<RdmaDevice, RdmaDeviceMax> { &self.devices }
}

/// HCA handle and object limits for one RDMA device, measured in counts.
/// See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RdmaDeviceMax {
  hca_handle: MaxOr<Count>,
  hca_object: MaxOr<Count>,
}

impl RdmaDeviceMax {
  /// Returns the stored HCA handle limit, or `max` for no limit here.
  #[must_use]
  pub const fn hca_handle(&self) -> MaxOr<Count> { self.hca_handle }

  /// Returns the stored HCA object limit, or `max` for no limit here.
  #[must_use]
  pub const fn hca_object(&self) -> MaxOr<Count> { self.hca_object }

  fn from_fields(fields: KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    Ok(Self {
      hca_handle: fields.required::<ParseCount, _>("hca_handle")?,
      hca_object: fields.required::<ParseCount, _>("hca_object")?,
    })
  }
}

impl FromStr for RdmaMax {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self { devices: parse_devices(contents, RdmaDeviceMax::from_fields)? })
  }
}

/// A volatile snapshot of hierarchical or local per-device RDMA events.
///
/// Since Linux 7.2; see the
/// [release-tagged implementation](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
/// Counts can increase between reads. An empty map means no devices were
/// reported; device removal removes its counters. Unknown resource keys are
/// ignored; optional device indices remain part of the map keys.
/// All four baseline event counters are required for each device.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RdmaEvents {
  devices: BTreeMap<RdmaDevice, RdmaDeviceEvents>,
}

impl RdmaEvents {
  /// The hierarchical `rdma.events` filename, available since Linux 7.2.
  pub const FILE_NAME: &'static str = "rdma.events";
  /// The local `rdma.events.local` filename, available since Linux 7.2.
  pub const LOCAL_FILE_NAME: &'static str = "rdma.events.local";

  /// Returns stored per-device event counters without performing I/O.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<RdmaDevice, RdmaDeviceEvents> { &self.devices }
}

/// Allocation events for both resource types of one RDMA device.
///
/// Since Linux 7.2; see the
/// [release-tagged implementation](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RdmaDeviceEvents {
  hca_handle: RdmaResourceEvents,
  hca_object: RdmaResourceEvents,
}

impl RdmaDeviceEvents {
  /// Returns stored HCA handle allocation-event counters.
  #[must_use]
  pub const fn hca_handle(&self) -> RdmaResourceEvents { self.hca_handle }

  /// Returns stored HCA object allocation-event counters.
  #[must_use]
  pub const fn hca_object(&self) -> RdmaResourceEvents { self.hca_object }

  fn from_fields(fields: KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    Ok(Self {
      hca_handle: RdmaResourceEvents {
        max: fields.required::<ParseCount, _>("hca_handle.max")?,
        alloc_fail: fields.required::<ParseCount, _>("hca_handle.alloc_fail")?,
      },
      hca_object: RdmaResourceEvents {
        max: fields.required::<ParseCount, _>("hca_object.max")?,
        alloc_fail: fields.required::<ParseCount, _>("hca_object.alloc_fail")?,
      },
    })
  }
}

/// Allocation-failure event counts for one RDMA resource.
///
/// Since Linux 7.2; see the
/// [release-tagged implementation](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RdmaResourceEvents {
  max: Count,
  alloc_fail: Count,
}

impl RdmaResourceEvents {
  /// Counts failures against this cgroup's own resource limit.
  ///
  /// Local counts include requests from descendants rejected by this limit.
  /// Hierarchical counts also include descendant cgroups' own limit events.
  #[must_use]
  pub const fn max(&self) -> Count { self.max }

  /// Counts allocation requests rejected by this cgroup's or an ancestor's limit.
  ///
  /// Local counts include requests originating in this cgroup. Hierarchical
  /// counts additionally include requests originating in descendants.
  #[must_use]
  pub const fn alloc_fail(&self) -> Count { self.alloc_fail }
}

impl FromStr for RdmaEvents {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self { devices: parse_devices(contents, RdmaDeviceEvents::from_fields)? })
  }
}

fn parse_devices<T>(
  raw: &str,
  parse: impl Fn(KeyedFields<'_>) -> Result<T, ParseError<ParseValueError>>,
) -> Result<BTreeMap<RdmaDevice, T>, ParseError<ParseValueError>> {
  raw
    .lines()
    .filter(|line| !line.trim().is_empty())
    .map(|line| {
      let mut tokens = line.split_ascii_whitespace();
      let device = tokens.next().ok_or_else(|| ParseError::missing(raw, "device"))?;
      let fields = KeyedFields::new(
        raw,
        tokens.map(|token| {
          token.split_once('=').ok_or_else(|| ParseError::invalid(raw, "value", token, ParseValueError::OutOfRange))
        }),
      )?;
      let index = fields
        .iter()
        .find_map(|(key, value)| (key == "index").then_some(value))
        .map(|value| value.parse::<u32>().map_err(|error| ParseError::invalid(raw, "index", value, error.into())))
        .transpose()?;
      Ok((RdmaDevice::new(device, index), parse(fields)?))
    })
    .collect()
}

#[cfg(test)]
mod tests {
  #[cfg(target_os = "linux")]
  use assertables::{assert_ok, assert_some};

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Invalid, Missing},
  };

  fn count(value: u64) -> Count { Count { value, ..Default::default() } }

  #[test]
  fn parses_usage_and_peak_snapshots() {
    Cases::<RdmaCurrent>::check([
      (
        include_str!("fixtures/p6/rdma.current"),
        Ok(RdmaCurrent {
          devices: BTreeMap::from([
            (RdmaDevice::new("mlx4_0", None), RdmaDeviceCurrent { hca_handle: count(1), hca_object: count(20) }),
            (RdmaDevice::new("ocrdma1", None), RdmaDeviceCurrent { hca_handle: count(0), hca_object: count(23) }),
          ]),
        }),
      ),
      (
        include_str!("fixtures/p6/rdma.peak"),
        Ok(RdmaCurrent {
          devices: BTreeMap::from([
            (RdmaDevice::new("mlx4_0", None), RdmaDeviceCurrent { hca_handle: count(2), hca_object: count(2000) }),
            (RdmaDevice::new("ocrdma1", None), RdmaDeviceCurrent { hca_handle: count(1), hca_object: count(23) }),
          ]),
        }),
      ),
      ("", Ok(RdmaCurrent::default())),
      (" \n\t", Ok(RdmaCurrent::default())),
      (
        "mlx4_0 hca_object=max future=nope hca_handle=max",
        Ok(RdmaCurrent {
          devices: BTreeMap::from([(RdmaDevice::new("mlx4_0", None), RdmaDeviceCurrent {
            hca_handle: count(i32::MAX as u64),
            hca_object: count(i32::MAX as u64),
          })]),
        }),
      ),
      (
        "mlx4_0 hca_handle=18446744073709551615 hca_object=0",
        Ok(RdmaCurrent {
          devices: BTreeMap::from([(RdmaDevice::new("mlx4_0", None), RdmaDeviceCurrent {
            hca_handle: count(u64::MAX),
            hca_object: count(0),
          })]),
        }),
      ),
      ("mlx4_0", Err(Missing("hca_handle"))),
      ("mlx4_0 hca_handle=1", Err(Missing("hca_object"))),
      ("mlx4_0 hca_handle", Err(Invalid("value", "hca_handle"))),
      ("mlx4_0 hca_handle=-1 hca_object=0", Err(Invalid("hca_handle", "-1"))),
      ("mlx4_0 hca_handle=0 hca_object=", Err(Invalid("hca_object", ""))),
      ("mlx4_0 hca_handle=0 hca_object=18446744073709551616", Err(Invalid("hca_object", "18446744073709551616"))),
    ]);
  }

  #[test]
  fn parses_limit_snapshots() {
    Cases::<RdmaMax>::check([
      (
        include_str!("fixtures/p6/rdma.max"),
        Ok(RdmaMax {
          devices: BTreeMap::from([
            (RdmaDevice::new("mlx4_0", None), RdmaDeviceMax {
              hca_handle: MaxOr::Value(count(2)),
              hca_object: MaxOr::Value(count(2000)),
            }),
            (RdmaDevice::new("ocrdma1", None), RdmaDeviceMax {
              hca_handle: MaxOr::Value(count(3)),
              hca_object: MaxOr::Max,
            }),
          ]),
        }),
      ),
      ("", Ok(RdmaMax::default())),
      (
        "mlx4_0 future=nope hca_object=max hca_handle=0",
        Ok(RdmaMax {
          devices: BTreeMap::from([(RdmaDevice::new("mlx4_0", None), RdmaDeviceMax {
            hca_handle: MaxOr::Value(count(0)),
            hca_object: MaxOr::Max,
          })]),
        }),
      ),
      (
        "mlx4_0 hca_handle=max hca_object=max",
        Ok(RdmaMax {
          devices: BTreeMap::from([(RdmaDevice::new("mlx4_0", None), RdmaDeviceMax {
            hca_handle: MaxOr::Max,
            hca_object: MaxOr::Max,
          })]),
        }),
      ),
      ("mlx4_0 hca_handle=max", Err(Missing("hca_object"))),
      ("mlx4_0 hca_handle=0 hca_object=nope", Err(Invalid("hca_object", "nope"))),
      ("mlx4_0 hca_handle=-1 hca_object=max", Err(Invalid("hca_handle", "-1"))),
      ("mlx4_0 hca_handle=18446744073709551616 hca_object=max", Err(Invalid("hca_handle", "18446744073709551616"))),
    ]);
  }

  #[test]
  fn parses_event_snapshots() {
    Cases::<RdmaEvents>::check([
      (
        include_str!("fixtures/p6/rdma.events"),
        Ok(RdmaEvents {
          devices: BTreeMap::from([(RdmaDevice::new("mlx4_0", None), RdmaDeviceEvents {
            hca_handle: RdmaResourceEvents { max: count(5), alloc_fail: count(7) },
            hca_object: RdmaResourceEvents { max: count(0), alloc_fail: count(1) },
          })]),
        }),
      ),
      (
        include_str!("fixtures/p6/rdma.events.local"),
        Ok(RdmaEvents {
          devices: BTreeMap::from([(RdmaDevice::new("mlx4_0", None), RdmaDeviceEvents {
            hca_handle: RdmaResourceEvents { max: count(2), alloc_fail: count(3) },
            hca_object: RdmaResourceEvents { max: count(0), alloc_fail: count(0) },
          })]),
        }),
      ),
      ("", Ok(RdmaEvents::default())),
      ("mlx4_0", Err(Missing("hca_handle.max"))),
      ("mlx4_0 hca_handle.max=0", Err(Missing("hca_handle.alloc_fail"))),
      ("mlx4_0 hca_handle.max=0 hca_handle.alloc_fail=0", Err(Missing("hca_object.max"))),
      ("mlx4_0 hca_handle.max=0 hca_handle.alloc_fail=0 hca_object.max=0", Err(Missing("hca_object.alloc_fail"))),
      ("mlx4_0 hca_handle.max=max", Err(Invalid("hca_handle.max", "max"))),
      ("mlx4_0 hca_handle.max=0 hca_handle.alloc_fail=-1", Err(Invalid("hca_handle.alloc_fail", "-1"))),
    ]);
  }

  #[test]
  fn preserves_same_named_devices_in_usage_limits_and_events() {
    let keys = [RdmaDevice::new("rxe0", Some(0)), RdmaDevice::new("rxe0", Some(5)), RdmaDevice::new("mlx4_0", None)];
    let usage = |values: [(u64, u64); 3]| RdmaCurrent {
      devices: keys
        .iter()
        .cloned()
        .zip(
          values.map(|(handles, objects)| RdmaDeviceCurrent { hca_handle: count(handles), hca_object: count(objects) }),
        )
        .collect(),
    };
    Cases::<RdmaCurrent>::check([
      (include_str!("fixtures/p6_rdma_index/rdma.current"), Ok(usage([(1, 20), (2, 30), (3, 40)]))),
      (include_str!("fixtures/p6_rdma_index/rdma.peak"), Ok(usage([(10, 200), (20, 300), (30, 400)]))),
    ]);
    Cases::<RdmaMax>::check([(
      include_str!("fixtures/p6_rdma_index/rdma.max"),
      Ok(RdmaMax {
        devices: BTreeMap::from([
          (RdmaDevice::new("rxe0", Some(0)), RdmaDeviceMax {
            hca_handle: MaxOr::Value(count(0)),
            hca_object: MaxOr::Value(count(20)),
          }),
          (RdmaDevice::new("rxe0", Some(5)), RdmaDeviceMax {
            hca_handle: MaxOr::Value(count(2)),
            hca_object: MaxOr::Max,
          }),
          (RdmaDevice::new("mlx4_0", None), RdmaDeviceMax {
            hca_handle: MaxOr::Max,
            hca_object: MaxOr::Value(count(40)),
          }),
        ]),
      }),
    )]);
    let events = |values: [[u64; 4]; 3]| RdmaEvents {
      devices: keys
        .iter()
        .cloned()
        .zip(values.map(|[handle_max, handle_fail, object_max, object_fail]| RdmaDeviceEvents {
          hca_handle: RdmaResourceEvents { max: count(handle_max), alloc_fail: count(handle_fail) },
          hca_object: RdmaResourceEvents { max: count(object_max), alloc_fail: count(object_fail) },
        }))
        .collect(),
    };
    Cases::<RdmaEvents>::check([
      (include_str!("fixtures/p6_rdma_index/rdma.events"), Ok(events([[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]]))),
      (
        include_str!("fixtures/p6_rdma_index/rdma.events.local"),
        Ok(events([[4, 5, 6, 7], [0, 1, 2, 3], [8, 9, 10, 11]])),
      ),
    ]);
  }

  #[test]
  fn validates_optional_device_indices_without_conflating_absence_with_zero() {
    for (suffix, expected) in [("", None), ("index=0", Some(0)), ("index=4294967295", Some(u32::MAX))] {
      let raw = format!("rxe0 {suffix} hca_handle=1 hca_object=2");
      Cases::<RdmaCurrent>::check([(
        raw.as_str(),
        Ok(RdmaCurrent {
          devices: BTreeMap::from([(RdmaDevice::new("rxe0", expected), RdmaDeviceCurrent {
            hca_handle: count(1),
            hca_object: count(2),
          })]),
        }),
      )]);
    }
    for index in ["", "-1", "max", "nope", "4294967296"] {
      let usage = format!("rxe0 index={index} hca_handle=1 hca_object=2");
      Cases::<RdmaCurrent>::check([(usage.as_str(), Err(Invalid("index", index)))]);
      Cases::<RdmaMax>::check([(usage.as_str(), Err(Invalid("index", index)))]);
      let events =
        format!("rxe0 index={index} hca_handle.max=0 hca_handle.alloc_fail=0 hca_object.max=0 hca_object.alloc_fail=0");
      Cases::<RdmaEvents>::check([(events.as_str(), Err(Invalid("index", index)))]);
    }
    let key = RdmaDevice::new("rxe0", Some(0));
    assert_eq!(key.name(), "rxe0");
    assert_eq!(key.index(), Some(0));
    assert_ne!(key, RdmaDevice::new("rxe0", None));
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn reads_rdma_fixtures_and_older_kernel_missing_files_without_writes() {
    for fixture in ["p6", "p6_rdma_index", "p6_older"] {
      let cgroup = Cgroup::fixture(&format!("fixtures/{fixture}"));
      let rdma = cgroup.rdma();
      let path = cgroup.path();
      let before: BTreeMap<_, _> = [
        RdmaCurrent::FILE_NAME,
        RdmaMax::FILE_NAME,
        RdmaCurrent::PEAK_FILE_NAME,
        RdmaEvents::FILE_NAME,
        RdmaEvents::LOCAL_FILE_NAME,
      ]
      .into_iter()
      .filter_map(|name| std::fs::read_to_string(path.join(name)).ok().map(|raw| (name, raw)))
      .collect();
      assert_eq!(
        assert_ok!(rdma.current()),
        assert_ok!(assert_some!(before.get(RdmaCurrent::FILE_NAME)).parse::<RdmaCurrent>())
      );
      assert_eq!(assert_ok!(rdma.max()), assert_ok!(assert_some!(before.get(RdmaMax::FILE_NAME)).parse::<RdmaMax>()));
      for (name, read) in [
        (RdmaCurrent::PEAK_FILE_NAME, rdma.peak().map(|_| ())),
        (RdmaEvents::FILE_NAME, rdma.events().map(|_| ())),
        (RdmaEvents::LOCAL_FILE_NAME, rdma.events_local().map(|_| ())),
      ] {
        match fixture {
          | "p6" | "p6_rdma_index" => {
            assert_ok!(read);
          },
          | _ => assert!(matches!(read, Err(Error::FileMissing { path: missing }) if missing == path.join(name))),
        }
      }
      for (name, raw) in before {
        assert_eq!(assert_ok!(std::fs::read_to_string(path.join(name))), raw);
      }
    }
    let cgroup = Cgroup::fixture("");
    for (name, read) in [
      (RdmaCurrent::FILE_NAME, cgroup.rdma().current().map(|_| ())),
      (RdmaMax::FILE_NAME, cgroup.rdma().max().map(|_| ())),
    ] {
      assert!(matches!(read, Err(Error::FileMissing { path: missing }) if missing == cgroup.path().join(name)));
    }
  }
}
