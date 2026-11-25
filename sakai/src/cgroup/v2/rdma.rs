//! Per-device RDMA resource accounting, limits, and allocation events.
//!
//! Device names identify RDMA/InfiniBand adapters rather than block devices.
//! Resources remain charged to their owning cgroup after task migration.

use std::collections::BTreeMap;

pub use crate::cgroup::common::rdma::{RdmaCurrent, RdmaDevice, RdmaDeviceCurrent, RdmaDeviceMax, RdmaMax};
use crate::{
  cgroup::common::rdma::{encode_device_max, parse_devices},
  error::{ParseError, ParseValueError},
  parse::{KeyedFields, ParseCount, cgroup_value, encode_pair},
  unit::Count,
  v2::interface::cgroup_file,
};

cgroup_file!(RdmaCurrent, "rdma.current", Read);
cgroup_file!(RdmaMax, "rdma.max", Read);
cgroup_file!(write RdmaMax, 'a => (&'a RdmaDevice, &'a RdmaDeviceMax), encode_device_max);
cgroup_file!(alias
  /// Peak RDMA usage.
  /// Since Linux 7.2. See the [Linux 7.2 source](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
  RdmaPeak, "rdma.peak" => RdmaCurrent
);

const HANDLE_MAX: &str = "hca_handle.max";
const HANDLE_ALLOC_FAIL: &str = "hca_handle.alloc_fail";
const OBJECT_MAX: &str = "hca_object.max";
const OBJECT_ALLOC_FAIL: &str = "hca_object.alloc_fail";

/// A volatile snapshot of hierarchical or local per-device RDMA events.
///
/// Since Linux 7.2; see the
/// [release-tagged implementation](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
/// Counts can increase between reads. An empty map means no devices were
/// reported; device removal removes its counters. Unknown resource keys are
/// ignored; optional device indices remain part of the map keys.
/// All four baseline event counters are required for each device.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct RdmaEvents {
  /// Stored per-device event counters without performing I/O.
  pub devices: BTreeMap<RdmaDevice, RdmaDeviceEvents>,
}

cgroup_value! { device_rows RdmaEvents, devices: RdmaDeviceEvents, parse_devices }
cgroup_file!(RdmaEvents, "rdma.events", Read);
cgroup_file!(alias
  /// Local RDMA events.
  /// Since Linux 7.2. See the [Linux 7.2 source](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
  RdmaEventsLocal, "rdma.events.local" => RdmaEvents
);

/// Allocation events for both resource types of one RDMA device.
///
/// Since Linux 7.2; see the
/// [release-tagged implementation](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RdmaDeviceEvents {
  /// Stored HCA handle allocation-event counters.
  pub hca_handle: RdmaResourceEvents,
  /// Stored HCA object allocation-event counters.
  pub hca_object: RdmaResourceEvents,
}

impl RdmaDeviceEvents {
  fn encode_fields(&self, output: &mut String, prefix: &str, delimiter: char) -> Result<(), crate::error::EncodeError> {
    for (key, value) in [
      (HANDLE_MAX, &self.hca_handle.max),
      (HANDLE_ALLOC_FAIL, &self.hca_handle.alloc_fail),
      (OBJECT_MAX, &self.hca_object.max),
      (OBJECT_ALLOC_FAIL, &self.hca_object.alloc_fail),
    ] {
      encode_pair::<ParseCount, _>(output, prefix, delimiter, key, value)?;
    }
    Ok(())
  }

  fn from_fields(fields: &KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    Ok(Self {
      hca_handle: RdmaResourceEvents {
        max: fields.required::<ParseCount, _>(HANDLE_MAX)?,
        alloc_fail: fields.required::<ParseCount, _>(HANDLE_ALLOC_FAIL)?,
      },
      hca_object: RdmaResourceEvents {
        max: fields.required::<ParseCount, _>(OBJECT_MAX)?,
        alloc_fail: fields.required::<ParseCount, _>(OBJECT_ALLOC_FAIL)?,
      },
    })
  }
}

/// Allocation-failure event counts for one RDMA resource.
///
/// Since Linux 7.2; see the
/// [release-tagged implementation](https://github.com/torvalds/linux/blob/v7.2/kernel/cgroup/rdma.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RdmaResourceEvents {
  /// Counts failures against this cgroup's own resource limit.
  ///
  /// Local counts include requests from descendants rejected by this limit.
  /// Hierarchical counts also include descendant cgroups' own limit events.
  pub max: Count,
  /// Counts allocation requests rejected by this cgroup's or an ancestor's limit.
  ///
  /// Local counts include requests originating in this cgroup. Hierarchical
  /// counts additionally include requests originating in descendants.
  pub alloc_fail: Count,
}

#[cfg(test)]
mod tests {
  use super::*;
  #[cfg(target_os = "linux")]
  use crate::{Cgroup, Error, v2::CgroupFile};
  use crate::{
    MaxOr,
    parse::tests::{
      Cases,
      Failure::{Invalid, Missing},
    },
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
    assert_eq!(key.name, "rxe0");
    assert_eq!(key.index, Some(0));
    assert_ne!(key, RdmaDevice::new("rxe0", None));
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn reads_rdma_fixtures_and_older_kernel_missing_files_without_writes() {
    for fixture in ["p6", "p6_rdma_index", "p6_older"] {
      let cgroup = Cgroup::fixture(&format!("fixtures/{fixture}"));
      let path = cgroup.path();
      // Also catch accidental writes to a different interface in this cgroup.
      let snapshot = || {
        [
          RdmaCurrent::FILE_NAME,
          RdmaMax::FILE_NAME,
          RdmaPeak::FILE_NAME,
          RdmaEvents::FILE_NAME,
          RdmaEventsLocal::FILE_NAME,
        ]
        .map(|name| std::fs::read_to_string(path.join(name)).ok())
      };
      let before = snapshot();
      cgroup.check_fixture(RdmaCurrent::FILE_NAME, false, || cgroup.read_value::<crate::v2::rdma::RdmaCurrent>());
      cgroup.check_fixture(RdmaMax::FILE_NAME, false, || cgroup.read_value::<crate::v2::rdma::RdmaMax>());
      match fixture {
        | "p6_older" => {
          for (name, read) in [
            (RdmaPeak::FILE_NAME, cgroup.read_value::<crate::v2::rdma::RdmaPeak>().map(|_| ())),
            (RdmaEvents::FILE_NAME, cgroup.read_value::<crate::v2::rdma::RdmaEvents>().map(|_| ())),
            (RdmaEventsLocal::FILE_NAME, cgroup.read_value::<crate::v2::rdma::RdmaEventsLocal>().map(|_| ())),
          ] {
            assert!(matches!(read, Err(Error::FileMissing { path: missing }) if missing == path.join(name)));
          }
        },
        | _ => {
          cgroup.check_fixture(RdmaPeak::FILE_NAME, false, || cgroup.read_value::<crate::v2::rdma::RdmaPeak>());
          cgroup.check_fixture(RdmaEvents::FILE_NAME, false, || cgroup.read_value::<crate::v2::rdma::RdmaEvents>());
          cgroup.check_fixture(RdmaEventsLocal::FILE_NAME, false, || {
            cgroup.read_value::<crate::v2::rdma::RdmaEventsLocal>()
          });
        },
      }
      assert_eq!(snapshot(), before);
    }
    let cgroup = Cgroup::fixture("");
    for (name, read) in [
      (RdmaCurrent::FILE_NAME, cgroup.read_value::<crate::v2::rdma::RdmaCurrent>().map(|_| ())),
      (RdmaMax::FILE_NAME, cgroup.read_value::<crate::v2::rdma::RdmaMax>().map(|_| ())),
    ] {
      assert!(matches!(read, Err(Error::FileMissing { path: missing }) if missing == cgroup.path().join(name)));
    }
  }
}
