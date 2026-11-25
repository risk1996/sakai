//! RDMA device keys, usage, and limit snapshots shared by both versions.

use std::{collections::BTreeMap, fmt};

use crate::{
  Count, MaxOr,
  error::{ParseError, ParseValueError},
  parse::{EncodeCgroup, KeyedFields, ParseCgroup, ParseCount, cgroup_value},
};

/// An RDMA adapter name and its optional system-wide device index.
///
/// Names can be shared by adapters in different network namespaces. Since
/// Linux 7.3, the kernel adds `index=N` when a name is
/// ambiguous; see the [Linux 7.3 implementation](https://github.com/torvalds/linux/blob/v7.3/kernel/cgroup/rdma.c).
/// Older kernels and globally unique names omit the index. `None` means
/// unreported, not index zero. Indices are unsigned identifiers, not counts.
///
/// This key describes one stored record, not a permanent device identity.
/// Renames, registration changes, and index reuse can change identity between
/// snapshots. An index can appear or disappear when another adapter shares
/// the name. See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct RdmaDevice {
  /// Adapter name.
  pub name: String,
  /// Device index, when reported.
  pub index: Option<u32>,
}

impl RdmaDevice {
  /// Constructs a record key without resolving a currently registered adapter.
  ///
  /// Preserve `None` when the kernel omitted the index; it differs from zero.
  #[must_use]
  pub fn new(name: impl Into<String>, index: Option<u32>) -> Self { Self { name: name.into(), index } }
}

const DEVICE_INDEX: &str = "index";

impl fmt::Display for RdmaDevice {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.name)?;
    if let Some(index) = self.index {
      write!(formatter, " {DEVICE_INDEX}={index}")?;
    }
    Ok(())
  }
}

struct UsageCount;

impl ParseCgroup<UsageCount> for Count {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    match <MaxOr<Count> as ParseCgroup<ParseCount>>::parse_cgroup(value)? {
      | MaxOr::Value(count) => Ok(count),
      | MaxOr::Max => Ok(Count { value: i32::MAX as u64, ..Default::default() }),
    }
  }
}

impl EncodeCgroup<UsageCount> for Count {
  fn encode_token(&self, output: &mut String) -> Option<()> { EncodeCgroup::<ParseCount>::encode_token(self, output) }
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
///
/// See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct RdmaCurrent {
  /// Stored per-device HCA counts without performing I/O.
  pub devices: BTreeMap<RdmaDevice, RdmaDeviceCurrent>,
}

cgroup_value! { device_rows RdmaCurrent, devices: RdmaDeviceCurrent, parse_devices }

/// Hierarchical HCA handle and object counts for one RDMA device.
///
/// HCA handles count userspace adapter contexts; HCA objects count resources
/// such as queue pairs, memory regions, and completion queues. These are
/// counts, not byte quantities. The kernel emits `max` when a usage or peak
/// count reaches its signed 32-bit maximum; this reader preserves that exact
/// count as 2,147,483,647 rather than treating usage as unlimited. See the
/// [kernel read implementation](https://github.com/torvalds/linux/blob/v5.15/kernel/cgroup/rdma.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RdmaDeviceCurrent {
  /// Stored number of HCA handles.
  pub hca_handle: Count,
  /// Stored number of HCA objects.
  pub hca_object: Count,
}

cgroup_value! { fields RdmaDeviceCurrent, {
  hca_handle: required UsageCount => "hca_handle",
  hca_object: required UsageCount => "hca_object",
} }

/// A per-device configuration snapshot from `rdma.max`, measured in counts.
///
/// Zero is a valid limit. `max` means no configured limit at this cgroup;
/// ancestor limits still apply. Configuration and registered devices can
/// change between reads. An empty map means no devices were reported.
/// Optional device indices are preserved in the map keys.
/// Unknown resource keys are ignored; the two baseline limits are required.
///
/// See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct RdmaMax {
  /// Resource limits by RDMA device.
  pub devices: BTreeMap<RdmaDevice, RdmaDeviceMax>,
}

cgroup_value! { device_rows RdmaMax, devices: RdmaDeviceMax, parse_devices }

/// HCA handle and object limits for one RDMA device, measured in counts.
///
/// See [Linux RDMA interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#rdma-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RdmaDeviceMax {
  /// HCA handle limit, or `max` for no limit here.
  pub hca_handle: MaxOr<Count>,
  /// HCA object limit, or `max` for no limit here.
  pub hca_object: MaxOr<Count>,
}

cgroup_value! { fields RdmaDeviceMax, {
  hca_handle: required ParseCount => "hca_handle",
  hca_object: required ParseCount => "hca_object",
} }

pub(in crate::cgroup) fn parse_devices<T>(
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
        .find_map(|(key, value)| (key == DEVICE_INDEX).then_some(value))
        .map(|value| value.parse::<u32>().map_err(|error| ParseError::invalid(raw, DEVICE_INDEX, value, error.into())))
        .transpose()?;
      Ok((RdmaDevice::new(device, index), parse(fields)?))
    })
    .collect()
}

pub(in crate::cgroup) fn encode_device_max(
  (device, value): (&RdmaDevice, &RdmaDeviceMax),
) -> Result<String, crate::error::EncodeError> {
  super::interface::check_token(&device.name, "device").and_then(|()| {
    let mut command = device.to_string();
    value.encode_fields(&mut command, " ", '=')?;
    Ok(command)
  })
}
