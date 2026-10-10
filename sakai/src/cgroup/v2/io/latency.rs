use std::{collections::BTreeMap, str::FromStr};

use super::Device;
use crate::{
  error::{ParseError, ParseValueError},
  parse::ParseNonZeroMicroseconds,
  unit::NonZeroTime,
};

/// A configuration snapshot of per-device `io.latency` protection targets.
///
/// Kernel targets are in microseconds, converted to nanosecond-backed time.
/// Only configured nonzero targets are emitted by the kernel: a disabled
/// target (written as zero) is absent from the map. An empty file is valid.
/// Targets protect a group relative to peers; they are not measured latencies
/// or guarantees. Targets and device membership can change between reads.
/// This optional interface requires kernel I/O latency controller support.
/// Unknown nested keys are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IoLatency {
  devices: BTreeMap<Device, NonZeroTime>,
}

impl IoLatency {
  /// The cgroup v2 `io.latency` interface filename.
  pub const FILE_NAME: &'static str = "io.latency";

  /// Returns configured latency targets keyed by block device.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<Device, NonZeroTime> { &self.devices }
}

impl FromStr for IoLatency {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self {
      devices: Device::parse_lines(contents, |fields| fields.required::<ParseNonZeroMicroseconds, _>("target"))?,
    })
  }
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;
  use uom::si::time::microsecond;

  use super::*;
  use crate::{
    parse::tests::{
      Cases,
      Failure::{Invalid, Missing},
    },
    unit::Time,
  };

  #[test]
  fn parses_latency_targets() {
    Cases::<IoLatency>::check([
      (
        include_str!("../fixtures/p3/io.latency"),
        Ok(IoLatency {
          devices: BTreeMap::from([
            (Device::new(8, 16), assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(25_000)))),
            (Device::new(8, 0), assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(1)))),
          ]),
        }),
      ),
      ("", Ok(IoLatency::default())),
      (" \n", Ok(IoLatency::default())),
      (
        "8:0 target=18446744073709551",
        Ok(IoLatency {
          devices: BTreeMap::from([(
            Device::new(8, 0),
            assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(18_446_744_073_709_551))),
          )]),
        }),
      ),
      ("8:0", Err(Missing("value"))),
      ("8:0 future=1", Err(Missing("target"))),
      ("8:0 target=0", Err(Invalid("target", "0"))),
      ("8:0 target=-1", Err(Invalid("target", "-1"))),
      ("8:0 target=max", Err(Invalid("target", "max"))),
      ("8:0 target=", Err(Invalid("target", ""))),
      ("8:0 target=18446744073709552", Err(Invalid("target", "18446744073709552"))),
      ("8:0 target=18446744073709551616", Err(Invalid("target", "18446744073709551616"))),
      ("8:0 target", Err(Invalid("value", "target"))),
      ("8:-1 target=1", Err(Invalid("device", "8:-1"))),
    ]);
  }
}
