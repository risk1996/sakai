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

/// A borrowed view of one cgroup's RDMA controller.
///
/// Each call reads a fresh, read-only snapshot. Separate calls are not atomic
/// together. Interfaces are absent on the hierarchy root and when the RDMA
/// controller is disabled or unavailable in the kernel configuration.
///
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
  /// Returns a fresh [`RdmaCurrent`] snapshot keyed by device name. See
  /// [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn current(&self) -> Result<RdmaCurrent, Error> { self.cgroup.parse(RdmaCurrent::FILE_NAME) }

  /// Reads per-device HCA handle and object limits, measured in counts.
  ///
  /// Returns a fresh [`RdmaMax`] configuration snapshot. `max` means no
  /// configured limit here; ancestor limits still apply. See
  /// [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
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
/// Counts include this cgroup and its descendants, keyed by registered device
/// name. An empty map means no devices were reported. Current usage can rise
/// or fall between reads; peaks are independent historical high-water marks
/// since cgroup creation. Device removal also removes its reported history.
/// Unknown resource keys are ignored; the two baseline counters are required.
///
/// See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RdmaCurrent {
  devices: BTreeMap<String, RdmaDeviceCurrent>,
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
  pub const fn devices(&self) -> &BTreeMap<String, RdmaDeviceCurrent> { &self.devices }
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
/// Unknown resource keys are ignored; the two baseline limits are required.
///
/// See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RdmaMax {
  devices: BTreeMap<String, RdmaDeviceMax>,
}

impl RdmaMax {
  /// The cgroup v2 `rdma.max` interface filename.
  pub const FILE_NAME: &'static str = "rdma.max";

  /// Returns stored per-device limits without performing I/O.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<String, RdmaDeviceMax> { &self.devices }
}

/// HCA handle and object limits for one RDMA device, measured in counts.
///
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
/// ignored; all four baseline event counters are required for each device.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RdmaEvents {
  devices: BTreeMap<String, RdmaDeviceEvents>,
}

impl RdmaEvents {
  /// The hierarchical `rdma.events` filename, available since Linux 7.2.
  pub const FILE_NAME: &'static str = "rdma.events";
  /// The local `rdma.events.local` filename, available since Linux 7.2.
  pub const LOCAL_FILE_NAME: &'static str = "rdma.events.local";

  /// Returns stored per-device event counters without performing I/O.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<String, RdmaDeviceEvents> { &self.devices }
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
) -> Result<BTreeMap<String, T>, ParseError<ParseValueError>> {
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
      Ok((device.to_owned(), parse(fields)?))
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
            ("mlx4_0".into(), RdmaDeviceCurrent { hca_handle: count(1), hca_object: count(20) }),
            ("ocrdma1".into(), RdmaDeviceCurrent { hca_handle: count(0), hca_object: count(23) }),
          ]),
        }),
      ),
      (
        include_str!("fixtures/p6/rdma.peak"),
        Ok(RdmaCurrent {
          devices: BTreeMap::from([
            ("mlx4_0".into(), RdmaDeviceCurrent { hca_handle: count(2), hca_object: count(2000) }),
            ("ocrdma1".into(), RdmaDeviceCurrent { hca_handle: count(1), hca_object: count(23) }),
          ]),
        }),
      ),
      ("", Ok(RdmaCurrent::default())),
      (" \n\t", Ok(RdmaCurrent::default())),
      (
        "mlx4_0 hca_object=max future=nope hca_handle=max",
        Ok(RdmaCurrent {
          devices: BTreeMap::from([("mlx4_0".into(), RdmaDeviceCurrent {
            hca_handle: count(i32::MAX as u64),
            hca_object: count(i32::MAX as u64),
          })]),
        }),
      ),
      (
        "mlx4_0 hca_handle=18446744073709551615 hca_object=0",
        Ok(RdmaCurrent {
          devices: BTreeMap::from([("mlx4_0".into(), RdmaDeviceCurrent {
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
            ("mlx4_0".into(), RdmaDeviceMax {
              hca_handle: MaxOr::Value(count(2)),
              hca_object: MaxOr::Value(count(2000)),
            }),
            ("ocrdma1".into(), RdmaDeviceMax { hca_handle: MaxOr::Value(count(3)), hca_object: MaxOr::Max }),
          ]),
        }),
      ),
      ("", Ok(RdmaMax::default())),
      (
        "mlx4_0 future=nope hca_object=max hca_handle=0",
        Ok(RdmaMax {
          devices: BTreeMap::from([("mlx4_0".into(), RdmaDeviceMax {
            hca_handle: MaxOr::Value(count(0)),
            hca_object: MaxOr::Max,
          })]),
        }),
      ),
      (
        "mlx4_0 hca_handle=max hca_object=max",
        Ok(RdmaMax {
          devices: BTreeMap::from([("mlx4_0".into(), RdmaDeviceMax {
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
          devices: BTreeMap::from([("mlx4_0".into(), RdmaDeviceEvents {
            hca_handle: RdmaResourceEvents { max: count(5), alloc_fail: count(7) },
            hca_object: RdmaResourceEvents { max: count(0), alloc_fail: count(1) },
          })]),
        }),
      ),
      (
        include_str!("fixtures/p6/rdma.events.local"),
        Ok(RdmaEvents {
          devices: BTreeMap::from([("mlx4_0".into(), RdmaDeviceEvents {
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

  #[cfg(target_os = "linux")]
  #[test]
  fn reads_rdma_fixtures_and_older_kernel_missing_files_without_writes() {
    for fixture in ["p6", "p6_older"] {
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
          | "p6" => {
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
