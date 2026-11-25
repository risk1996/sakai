//! Traffic-control class identifiers.
//!
//! See the [Linux v1 network classifier guide](https://docs.kernel.org/admin-guide/cgroup-v1/net_cls.html).

use std::str::FromStr;

use super::codec::Unsigned;
use crate::{
  cgroup::common::interface::cgroup_file,
  error::{ParseError, ParseValueError},
  parse::Parser,
};

/// Traffic-control class for packets originating from this cgroup.
/// The kernel reports the packed 32-bit identifier in decimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ClassId {
  /// Upper 16 bits of the traffic-control class.
  pub major: u16,
  /// Lower 16 bits of the traffic-control class.
  pub minor: u16,
}

impl ClassId {
  /// Constructs a class identifier from its major and minor components.
  #[must_use]
  pub const fn new(major: u16, minor: u16) -> Self { Self { major, minor } }
}

impl FromStr for ClassId {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let packed = Parser::single::<Unsigned, u32>(contents, "classid")?;
    let [hi, lo, mi, ni] = packed.to_be_bytes();
    Ok(Self::new(u16::from_be_bytes([hi, lo]), u16::from_be_bytes([mi, ni])))
  }
}

cgroup_file!(ClassId, "net_cls.classid", Read; super::Version);
cgroup_file!(write ClassId, 'a => ClassId, |id: ClassId| Ok(format!("0x{:04x}{:04x}", id.major, id.minor)));
