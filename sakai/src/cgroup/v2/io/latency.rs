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
///
/// See [Linux `io.latency` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IoLatency {
  /// Configured latency targets by block device.
  pub devices: BTreeMap<Device, NonZeroTime>,
}

crate::v2::interface::cgroup_file!(file IoLatency, "io.latency", ReadWrite);
crate::v2::interface::cgroup_file!(read IoLatency => IoLatency);

impl IoLatency {
  /// The cgroup v2 `io.latency` interface filename.
  ///
  /// See [Linux `io.latency` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;

  /// Returns configured latency targets keyed by block device.
  ///
  /// See [Linux io interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-interface-files).
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<Device, NonZeroTime> { &self.devices }
}

const TARGET: &str = "target";

impl IoLatency {
  /// Encodes the stored value as canonical cgroup text.
  ///
  /// # Errors
  ///
  /// Returns an error if a field cannot be represented exactly.
  pub fn encode_cgroup(&self) -> Result<String, crate::error::EncodeError> {
    let mut output = String::new();
    for (device, target) in &self.devices {
      if !output.is_empty() {
        output.push('\n');
      }
      output.push_str(&device.to_string());
      crate::parse::encode_pair::<ParseNonZeroMicroseconds, _>(&mut output, " ", '=', TARGET, target)?;
    }
    Ok(output)
  }
}

#[cfg(target_os = "linux")]
impl super::Io<'_> {
  /// Sets a device's latency target in whole microseconds. Pass `None` to clear it.
  /// See [Linux I/O latency](https://docs.kernel.org/admin-guide/cgroup-v2.html#io-latency-interface-files).
  ///
  /// # Errors
  /// A target must be exactly representable in microseconds. See the
  /// shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_latency(&self, device: Device, target: Option<NonZeroTime>) -> Result<(), crate::Error> {
    self.cgroup.write_value::<IoLatency>((device, target))
  }
}

impl FromStr for IoLatency {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Ok(Self {
      devices: Device::parse_lines(contents, |fields| fields.required::<ParseNonZeroMicroseconds, _>(TARGET))?,
    })
  }
}

crate::v2::interface::cgroup_file!(write IoLatency, 'a => (Device, Option<NonZeroTime>), encode_target);

fn encode_target((device, target): (Device, Option<NonZeroTime>)) -> Result<String, crate::error::EncodeError> {
  let mut command = device.to_string();
  let duration = target.map_or_else(crate::Time::default, NonZeroTime::into_inner);
  let encoded =
    crate::parse::encode_pair::<crate::parse::ParseMicroseconds, _>(&mut command, " ", '=', TARGET, &duration);
  encoded.map(|()| command)
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
