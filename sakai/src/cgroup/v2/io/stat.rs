use std::{collections::BTreeMap, str::FromStr};

use super::Device;
use crate::{
  error::{ParseError, ParseValueError},
  parse::{KeyedFields, ParseBytes, ParseCount},
  unit::{Bytes, Count},
};

/// A volatile snapshot of hierarchical `io.stat` counters, keyed by device.
///
/// Bytes and operation counts accumulate independently; reads are not atomic
/// across devices. An empty file means no devices were reported. Older kernels
/// can omit discard counters or emit rows containing only controller debug
/// statistics. Missing counters remain `None`, and unknown keys (including
/// latency and cost-controller debug statistics) are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IoStat {
  devices: BTreeMap<Device, IoDeviceStat>,
}

impl IoStat {
  /// The cgroup v2 `io.stat` interface filename.
  pub const FILE_NAME: &'static str = "io.stat";

  /// Returns the byte and operation counters for each reported device.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<Device, IoDeviceStat> { &self.devices }
}

/// Cumulative byte and operation counters for one block device.
///
/// Absent fields are not assumed to be zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IoDeviceStat {
  read_bytes: Option<Bytes>,
  write_bytes: Option<Bytes>,
  discard_bytes: Option<Bytes>,
  read_operations: Option<Count>,
  write_operations: Option<Count>,
  discard_operations: Option<Count>,
}

impl IoDeviceStat {
  /// Cumulative bytes read (`rbytes`).
  #[must_use]
  pub const fn read_bytes(self) -> Option<Bytes> { self.read_bytes }

  /// Cumulative bytes written (`wbytes`).
  #[must_use]
  pub const fn write_bytes(self) -> Option<Bytes> { self.write_bytes }

  /// Cumulative bytes discarded (`dbytes`), when reported.
  #[must_use]
  pub const fn discard_bytes(self) -> Option<Bytes> { self.discard_bytes }

  /// Cumulative read operations (`rios`).
  #[must_use]
  pub const fn read_operations(self) -> Option<Count> { self.read_operations }

  /// Cumulative write operations (`wios`).
  #[must_use]
  pub const fn write_operations(self) -> Option<Count> { self.write_operations }

  /// Cumulative discard operations (`dios`), when reported.
  #[must_use]
  pub const fn discard_operations(self) -> Option<Count> { self.discard_operations }

  fn from_fields(fields: KeyedFields<'_>) -> Result<Self, ParseError<ParseValueError>> {
    Ok(Self {
      read_bytes: fields.optional::<ParseBytes, _>("rbytes")?,
      write_bytes: fields.optional::<ParseBytes, _>("wbytes")?,
      discard_bytes: fields.optional::<ParseBytes, _>("dbytes")?,
      read_operations: fields.optional::<ParseCount, _>("rios")?,
      write_operations: fields.optional::<ParseCount, _>("wios")?,
      discard_operations: fields.optional::<ParseCount, _>("dios")?,
    })
  }
}

impl FromStr for IoStat {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self { devices: Device::parse_lines(contents, IoDeviceStat::from_fields)? })
  }
}

#[cfg(test)]
mod tests {
  use uom::si::information::byte;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Invalid, Missing},
  };

  impl IoDeviceStat {
    fn expected(bytes: [u64; 2], operations: [u64; 2], discard: Option<(u64, u64)>) -> Self {
      let [read_bytes, write_bytes] = bytes.map(|value| Some(Bytes::new::<byte>(value)));
      let [read_operations, write_operations] = operations.map(|value| Some(Count { value, ..Default::default() }));
      Self {
        read_bytes,
        write_bytes,
        read_operations,
        write_operations,
        discard_bytes: discard.map(|(value, _)| Bytes::new::<byte>(value)),
        discard_operations: discard.map(|(_, value)| Count { value, ..Default::default() }),
      }
    }
  }

  #[test]
  fn parses_stat_snapshots() {
    Cases::<IoStat>::check([
      (
        include_str!("../fixtures/p3/io.stat"),
        Ok(IoStat {
          devices: BTreeMap::from([
            (Device::new(8, 16), IoDeviceStat::expected([1_459_200, 314_773_504], [192, 353], Some((0, 0)))),
            (
              Device::new(8, 0),
              IoDeviceStat::expected([90_430_464, 299_008_000], [8950, 1252], Some((50_331_648, 3021))),
            ),
          ]),
        }),
      ),
      (
        include_str!("../fixtures/p3_older/io.stat"),
        Ok(IoStat {
          devices: BTreeMap::from([
            (Device::new(8, 0), IoDeviceStat::expected([0, u64::MAX], [0, u64::MAX], None)),
            (Device::new(8, 16), IoDeviceStat::default()),
          ]),
        }),
      ),
      ("", Ok(IoStat::default())),
      (" \n\t", Ok(IoStat::default())),
      ("8:0", Err(Missing("value"))),
      ("8:0 rbytes", Err(Invalid("value", "rbytes"))),
      ("8:0 rbytes=", Err(Invalid("rbytes", ""))),
      ("8:0 rbytes=-1", Err(Invalid("rbytes", "-1"))),
      ("8:0 wbytes=nope", Err(Invalid("wbytes", "nope"))),
      ("8:0 dbytes=18446744073709551616", Err(Invalid("dbytes", "18446744073709551616"))),
      ("8:0 rios=-1", Err(Invalid("rios", "-1"))),
      ("8:0 wios=18446744073709551616", Err(Invalid("wios", "18446744073709551616"))),
      ("8:0 dios=nope", Err(Invalid("dios", "nope"))),
      ("sda rbytes=1", Err(Invalid("device", "sda"))),
    ]);
  }
}
