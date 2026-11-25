pub use crate::cgroup::common::{CpuList, MemoryNodeList};
use crate::{cgroup::common::cpuset::placement_command, v2::interface::cgroup_file};

cgroup_file!(CpuList, "cpuset.cpus", Read);
cgroup_file!(write CpuList, 'a => &'a CpuList, |value: &CpuList| placement_command(value.encode_cgroup()));
cgroup_file!(MemoryNodeList, "cpuset.mems", Read);
cgroup_file!(write MemoryNodeList, 'a => &'a MemoryNodeList, |value: &MemoryNodeList| placement_command(value.encode_cgroup()));

#[cfg(test)]
mod tests {
  use assertables::assert_ok;

  use super::*;
  use crate::parse::tests::{Cases, Failure::Invalid};

  #[test]
  fn parses_placement_lists_and_rejects_malformed_ranges() {
    for (raw, expected) in [
      ("", vec![]),
      (" \n", vec![]),
      ("0\n", std::iter::once(0..=0).collect()),
      (include_str!("../fixtures/p4/cpuset.cpus"), vec![0..=3, 5..=5, 7..=9]),
      (include_str!("../fixtures/p4/cpuset.mems"), vec![0..=1, 3..=3]),
      (include_str!("../fixtures/p4_older/cpuset.cpus"), vec![]),
      (" 0-3,5,7-9\n", vec![0..=3, 5..=5, 7..=9]),
      ("1,2-2,3", vec![1..=1, 2..=2, 3..=3]),
      ("0-4294967295", std::iter::once(0..=u32::MAX).collect()),
      ("4294967295", std::iter::once(u32::MAX..=u32::MAX).collect()),
    ] {
      Cases::<CpuList>::check([(raw, Ok(CpuList { ranges: expected.clone() }))]);
      Cases::<MemoryNodeList>::check([(raw, Ok(MemoryNodeList { ranges: expected }))]);
    }
    for (raw, bad) in [
      (",", ""),
      ("1,", ""),
      (",1", ""),
      ("1,,2", ""),
      ("-1", "-1"),
      ("+1", "+1"),
      ("1-", "1-"),
      ("1--2", "1--2"),
      ("3-1", "3-1"),
      ("1-2-3", "1-2-3"),
      ("1 2", "1 2"),
      ("1, 2", " 2"),
      ("1 - 2", "1 - 2"),
      ("max", "max"),
      ("4294967296", "4294967296"),
      ("0-4294967296", "0-4294967296"),
      ("2,1", "2,1"),
      ("1,1", "1,1"),
      ("0-2,2-3", "0-2,2-3"),
    ] {
      Cases::<CpuList>::check([(raw, Err(Invalid("cpus", bad)))]);
      Cases::<MemoryNodeList>::check([(raw, Err(Invalid("mems", bad)))]);
    }
  }

  #[test]
  fn inspects_stored_ids_without_expanding_ranges() {
    let cpus = assert_ok!("0-2,5,4294967295".parse::<CpuList>());
    assert_eq!(cpus.ids().collect::<Vec<_>>(), vec![0, 1, 2, 5, u32::MAX]);
    for (id, expected) in [(0, true), (2, true), (3, false), (5, true), (u32::MAX, true)] {
      assert_eq!(cpus.contains(id), expected);
    }
    let nodes = assert_ok!("0-1,4".parse::<MemoryNodeList>());
    assert_eq!(nodes.ids().collect::<Vec<_>>(), vec![0, 1, 4]);
    assert!(nodes.contains(4));
    assert!(!nodes.contains(3));
    assert_eq!(CpuList::default().ids().next(), None);
    assert!(!MemoryNodeList::default().contains(0));
  }
}
