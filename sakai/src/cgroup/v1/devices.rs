//! Device whitelist snapshots and rule changes.
//!
//! See the [Linux v1 device controller guide](https://docs.kernel.org/admin-guide/cgroup-v1/devices.html).

use std::{fmt, str::FromStr};

use crate::{
  error::{ParseError, ParseValueError},
  parse::Parser,
};

/// Device type selected by a whitelist rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display)]
#[non_exhaustive]
pub enum DeviceKind {
  /// All device types.
  #[strum(serialize = "a")]
  All,
  /// Block devices.
  #[strum(serialize = "b")]
  Block,
  /// Character devices.
  #[strum(serialize = "c")]
  Character,
}

/// One device whitelist entry. Numbers omitted by the rule are wildcards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeviceRule {
  /// Device type.
  pub kind: DeviceKind,
  /// Exact major number, or None to match any.
  pub major: Option<u32>,
  /// Exact minor number, or None to match any.
  pub minor: Option<u32>,
  /// Whether the rule applies to read access.
  pub read: bool,
  /// Whether the rule applies to write access.
  pub write: bool,
  /// Whether the rule applies to device node creation.
  pub mknod: bool,
}

impl FromStr for DeviceRule {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      let kind = parser.next_raw_field("kind")?;
      let kind =
        kind.parse().map_err(|error| ParseError::invalid(contents, "kind", kind, ParseValueError::from(error)))?;
      let device = parser.next_raw_field("device")?;
      let invalid_device = || ParseError::invalid(contents, "device", device, ParseValueError::OutOfRange);
      let (major, minor) = device.split_once(':').ok_or_else(invalid_device)?;
      let number = |value: &str| match value {
        | "*" => Ok(None),
        | value => value.parse::<u32>().ok().filter(|&number| number != u32::MAX).map(Some).ok_or_else(invalid_device),
      };
      let access = parser.next_raw_field("access")?;
      let invalid_access = || ParseError::invalid(contents, "access", access, ParseValueError::OutOfRange);
      let (mut read, mut write, mut mknod) = (false, false, false);
      for permission in access.bytes() {
        let flag = match permission {
          | b'r' => &mut read,
          | b'w' => &mut write,
          | b'm' => &mut mknod,
          | _ => return Err(invalid_access()),
        };
        if *flag {
          return Err(invalid_access());
        }
        *flag = true;
      }
      let rule = Self { kind, major: number(major)?, minor: number(minor)?, read, write, mknod };
      if kind == DeviceKind::All && !rule.is_all_devices() {
        return Err(invalid_device());
      }
      Ok(rule)
    })
  }
}

impl DeviceRule {
  fn is_all_devices(&self) -> bool {
    self.major.is_none() && self.minor.is_none() && self.read && self.write && self.mknod
  }

  fn encode(self) -> Result<String, crate::error::EncodeError> {
    if self.major == Some(u32::MAX)
      || self.minor == Some(u32::MAX)
      || (self.kind == DeviceKind::All && !self.is_all_devices())
    {
      return Err(crate::error::EncodeError { field: "device" });
    }
    if !(self.read || self.write || self.mknod) {
      return Err(crate::error::EncodeError { field: "access" });
    }
    Ok(self.to_string())
  }
}

impl fmt::Display for DeviceRule {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{} ", self.kind)?;
    for (number, separator) in [(self.major, ":"), (self.minor, " ")] {
      match number {
        | Some(number) => write!(f, "{number}{separator}")?,
        | None => write!(f, "*{separator}")?,
      }
    }
    for (enabled, permission) in [(self.read, "r"), (self.write, "w"), (self.mknod, "m")] {
      if enabled {
        f.write_str(permission)?;
      }
    }
    Ok(())
  }
}

v1_file!(alias
  /// Current allowlist, preserving row order.
  DevicesList, "devices.list" => Vec<DeviceRule>,
  |contents: &str| contents.lines().filter(|line| !line.trim().is_empty()).map(str::parse).collect()
);

/// Write one rule to allow device access within the limits of the parent cgroup.
#[non_exhaustive]
pub struct DevicesAllow;

v1_file!(file DevicesAllow, "devices.allow");
v1_file!(write DevicesAllow, 'a => DeviceRule, DeviceRule::encode);

/// Write one rule to remove device access.
#[non_exhaustive]
pub struct DevicesDeny;

v1_file!(file DevicesDeny, "devices.deny");
v1_file!(write DevicesDeny, 'a => DeviceRule, DeviceRule::encode);

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};

  use super::*;
  use crate::cgroup::{ReadableFile, WritableFile};

  #[test]
  fn parses_and_preserves_wildcard_rules() {
    let rule =
      DeviceRule { kind: DeviceKind::Character, major: Some(1), minor: None, read: true, write: false, mknod: true };
    assert_eq!(assert_ok!(DevicesList::decode("c 1:* rm\n")), [rule]);
    assert_eq!(assert_ok!(DevicesAllow::encode(rule)), "c 1:* rm");
    assert_eq!(assert_ok!(DevicesDeny::encode(rule)), "c 1:* rm");
    for bad in ["c 1:* rr", "c 1:* rz", "c 4294967295:* rm", "a 1:2 rwm", "b 1:*"] {
      assert_err!(bad.parse::<DeviceRule>());
    }
    assert_err!(DevicesAllow::encode(DeviceRule { major: Some(u32::MAX), ..rule }));
  }
}
