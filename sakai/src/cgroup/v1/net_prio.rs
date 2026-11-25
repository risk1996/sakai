//! Network interface priority assignments.
//!
//! See the [Linux v1 network priority guide](https://docs.kernel.org/admin-guide/cgroup-v1/net_prio.html).

use std::collections::BTreeMap;

use super::codec::Unsigned;
use crate::{
  cgroup::common::interface::{cgroup_file, cgroup_scalars, encode_keyed},
  parse::cgroup_value,
};

cgroup_scalars! {
  super::Version;
  /// Kernel's priority index for this cgroup.
  PriorityIndex, "net_prio.prioidx", u64, Unsigned, Read;
}

/// Configured priorities by interface name. The list reflects visible interfaces at read time.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InterfacePriorities {
  /// Unsigned packet priority per interface.
  pub priorities: BTreeMap<String, u32>,
}

cgroup_value!(map InterfacePriorities, priorities: Unsigned, "priority");
cgroup_file!(InterfacePriorities, "net_prio.ifpriomap", Read; super::Version);
cgroup_file!(write InterfacePriorities, 'a => (&'a str, u32), |(interface, priority): (&str, u32)| {
  encode_keyed::<Unsigned>(interface, "interface", &priority, "priority")
});
