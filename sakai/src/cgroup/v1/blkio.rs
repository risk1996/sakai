//! Per-device block I/O accounting and throttling.
//!
//! See the [Linux v1 block I/O guide](https://docs.kernel.org/admin-guide/cgroup-v1/blkio-controller.html).

use std::collections::BTreeMap;

use super::codec::Unsigned;
pub use crate::v2::io::Device;
use crate::{
  Bytes, BytesPerSecond, Count, OperationsPerSecond,
  error::{ParseError, ParseValueError},
  parse::{EncodeCgroup, ParseBandwidth, ParseBytes, ParseCgroup, ParseCount, ParseIops, Parser},
};

v1_scalars! {
  /// Default block I/O scheduling weight. Kernel support depends on the I/O scheduler.
  BlkioWeight, "blkio.weight", u64, Unsigned, ReadWrite;
}

/// Operation category in block I/O accounting. The categories overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, strum::EnumString)]
#[non_exhaustive]
pub enum Operation {
  /// Read I/O.
  Read,
  /// Write I/O.
  Write,
  /// Device discard operations.
  Discard,
  /// Synchronous I/O.
  Sync,
  /// Asynchronous I/O.
  Async,
  /// All operations for the device.
  Total,
}

/// Volatile snapshot of per-device accounting, with an optional grand total.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IoStat<T> {
  /// Counters grouped by device and operation.
  pub devices: BTreeMap<Device, BTreeMap<Operation, T>>,
  /// Total across devices if reported by the kernel.
  pub total: Option<T>,
}

impl<T> IoStat<T> {
  fn parse<Unit>(contents: &str) -> Result<Self, ParseError<ParseValueError>>
  where
    T: ParseCgroup<Unit, Error = ParseValueError>, {
    let mut stat = Self { devices: BTreeMap::new(), total: None };
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
      Parser::parse_line(contents, line, |parser| {
        let first = parser.next_raw_field("device")?;
        if first == "Total" {
          stat.total = Some(parser.next_field::<Unit, T>("total")?);
        } else {
          let device = first.parse::<Device>()?;
          let operation = parser.next_raw_field("operation")?;
          let value = parser.next_field::<Unit, T>("value")?;
          if let Ok(operation) = operation.parse::<Operation>() {
            stat.devices.entry(device).or_default().insert(operation, value);
          }
        }
        Ok(())
      })?;
    }
    Ok(stat)
  }
}

v1_file!(alias
  /// Bytes transferred per device and operation using the throttling interface.
  ThrottleIoServiceBytes, "blkio.throttle.io_service_bytes" => IoStat<Bytes>, IoStat::parse::<ParseBytes>
);
v1_file!(alias
  /// Bios issued per device and operation using the throttling interface.
  ThrottleIoServiced, "blkio.throttle.io_serviced" => IoStat<Count>, IoStat::parse::<ParseCount>
);
v1_file!(alias
  /// Bytes transferred per device and operation, when supported by the scheduler.
  IoServiceBytes, "blkio.io_service_bytes" => IoStat<Bytes>, IoStat::parse::<ParseBytes>
);
v1_file!(alias
  /// I/O requests issued per device and operation, when supported by the scheduler.
  IoServiced, "blkio.io_serviced" => IoStat<Count>, IoStat::parse::<ParseCount>
);

macro_rules! throttle_files {
  ($($(#[$doc:meta])* $name:ident, $file:literal, $value:ty, $codec:ty;)+) => {
    $(
      $(#[$doc])*
      #[non_exhaustive]
      pub struct $name;

      v1_file!(file $name, $file);
      v1_file!(read $name => BTreeMap<Device, $value>, |contents: &str| {
        contents.lines().filter(|line| !line.trim().is_empty()).map(|line| {
          Parser::parse_line(contents, line, |parser| {
            let device = parser.next_raw_field("device")?.parse::<Device>()?;
            let rate = parser.next_field::<$codec, $value>("rate")?;
            Ok((device, rate))
          })
        }).collect()
      });
      v1_file!(write $name, 'a => (Device, $value), |(device, value)| {
        let mut command = format!("{device} ");
        EncodeCgroup::<$codec>::encode_field(&value, &mut command, "rate")?;
        Ok(command)
      });
    )+
  };
}

throttle_files! {
  /// Read limit per device in bytes per second. Writing zero removes the limit.
  ThrottleReadBps, "blkio.throttle.read_bps_device", BytesPerSecond, ParseBandwidth;
  /// Write limit per device in bytes per second. Writing zero removes the limit.
  ThrottleWriteBps, "blkio.throttle.write_bps_device", BytesPerSecond, ParseBandwidth;
  /// Read operations per second per device. Writing zero removes the limit.
  ThrottleReadIops, "blkio.throttle.read_iops_device", OperationsPerSecond, ParseIops;
  /// Write operations per second per device. Writing zero removes the limit.
  ThrottleWriteIops, "blkio.throttle.write_iops_device", OperationsPerSecond, ParseIops;
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;
  use uom::si::{information::byte, information_rate::byte_per_second};

  use super::*;
  use crate::cgroup::{ReadableFile, WritableFile};

  #[test]
  fn preserves_each_device_and_the_independent_grand_total() {
    let disk = Device::new(8, 0);
    let input = "8:0 Read 200\n8:0 Write 100\n8:0 Sync 300\n8:0 Total 300\nTotal 400\n";
    assert_eq!(assert_ok!(ThrottleIoServiceBytes::decode(input)), IoStat {
      devices: BTreeMap::from([(
        disk,
        BTreeMap::from([
          (Operation::Read, Bytes::new::<byte>(200)),
          (Operation::Write, Bytes::new::<byte>(100)),
          (Operation::Sync, Bytes::new::<byte>(300)),
          (Operation::Total, Bytes::new::<byte>(300)),
        ])
      )]),
      total: Some(Bytes::new::<byte>(400)),
    });
    let limit = BytesPerSecond::new::<byte_per_second>(512);
    assert_eq!(assert_ok!(ThrottleReadBps::decode("8:0 512\n")), BTreeMap::from([(disk, limit)]));
    assert_eq!(assert_ok!(ThrottleReadBps::encode((disk, limit))), "8:0 512");
    assert_eq!(assert_ok!(ThrottleReadBps::encode((disk, BytesPerSecond::default()))), "8:0 0");
  }
}
