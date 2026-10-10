use std::{collections::BTreeMap, str::FromStr};

use nutype::nutype;

use super::Device;
use crate::{
  error::{ParseError, ParseValueError},
  parse::{KeyedFields, ParseCgroup},
};

/// An I/O scheduling weight in the kernel-supported range, 1 through 10,000.
///
/// Weights are dimensionless and specify relative I/O time among siblings.
#[nutype(
  validate(greater_or_equal = 1, less_or_equal = 10_000),
  derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRef, Deref)
)]
pub struct Weight(u16);

/// A configuration snapshot of `io.weight`'s default and device overrides.
///
/// The default applies to devices without an override. Only explicit overrides
/// are reported; the map is not an inventory of devices. Configuration can
/// change between reads. Unlike writes, read output always has a `default`
/// line and numeric overrides; the write-only reset token is not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoWeight {
  default: Weight,
  devices: BTreeMap<Device, Weight>,
}

impl IoWeight {
  /// The cgroup v2 `io.weight` interface filename.
  pub const FILE_NAME: &'static str = "io.weight";

  /// Returns the default scheduling weight.
  #[must_use]
  pub const fn default_weight(&self) -> Weight { self.default }

  /// Returns explicit per-device scheduling-weight overrides.
  #[must_use]
  pub const fn devices(&self) -> &BTreeMap<Device, Weight> { &self.devices }
}

impl FromStr for IoWeight {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let fields = KeyedFields::parse(contents, |_| "weight")?;
    let default = fields.required::<ParseWeight, _>("default")?;
    let devices = fields
      .iter()
      .filter(|(key, _)| *key != "default")
      .map(|(key, value)| Ok((Device::parse(contents, key)?, Weight::parse_field(contents, "weight", value)?)))
      .collect::<Result<_, Self::Err>>()?;
    Ok(Self { default, devices })
  }
}

struct ParseWeight;

impl ParseCgroup<ParseWeight> for Weight {
  type Error = ParseValueError;

  fn parse_cgroup(value: &str) -> Result<Self, Self::Error> {
    Self::try_new(value.parse()?).map_err(|_error| ParseValueError::OutOfRange)
  }
}

#[cfg(test)]
mod tests {
  use assertables::assert_ok;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

  #[test]
  fn parses_default_and_device_weights() {
    Cases::<IoWeight>::check([
      (
        include_str!("../fixtures/p3/io.weight"),
        Ok(IoWeight {
          default: assert_ok!(Weight::try_new(100)),
          devices: BTreeMap::from([
            (Device::new(8, 16), assert_ok!(Weight::try_new(200))),
            (Device::new(8, 0), assert_ok!(Weight::try_new(50))),
          ]),
        }),
      ),
      (
        include_str!("../fixtures/p3_older/io.weight"),
        Ok(IoWeight { default: assert_ok!(Weight::try_new(100)), devices: BTreeMap::new() }),
      ),
      (
        "8:0 10000\ndefault 1",
        Ok(IoWeight {
          default: assert_ok!(Weight::try_new(1)),
          devices: BTreeMap::from([(Device::new(8, 0), assert_ok!(Weight::try_new(10_000)))]),
        }),
      ),
      ("", Err(Missing("default"))),
      ("8:0 100", Err(Missing("default"))),
      ("default", Err(Missing("weight"))),
      ("default 0", Err(Invalid("default", "0"))),
      ("default 10001", Err(Invalid("default", "10001"))),
      ("default -1", Err(Invalid("default", "-1"))),
      ("default nope", Err(Invalid("default", "nope"))),
      ("default 65536", Err(Invalid("default", "65536"))),
      ("default 100\n8:0 0", Err(Invalid("weight", "0"))),
      ("default 100\n8:0 10001", Err(Invalid("weight", "10001"))),
      ("default 100\n8:0 default", Err(Invalid("weight", "default"))),
      ("default 100\nsda 10", Err(Invalid("device", "sda"))),
      ("default 100 extra", Err(Excess)),
      ("default 100\n8:0", Err(Missing("weight"))),
    ]);
  }
}
