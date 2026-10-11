//! CPU and NUMA memory-node placement snapshots.
//!
//! Configured lists express requests for tasks in this cgroup; effective lists
//! describe the online resources granted by its parent. These are identifiers,
//! not counts or quantities. Reads never change placement or migrate memory.
//! See [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).

pub use list::{CpuList, MemoryNodeList};
pub use partition::CpusetPartition;

#[cfg(target_os = "linux")]
use super::Cgroup;
#[cfg(target_os = "linux")]
use crate::Error;

mod list;
mod partition;

/// A borrowed reader for one cgroup's CPU and memory placement.
///
/// Each method opens a fresh read-only descriptor; separate reads are not
/// atomic together. Controller availability and kernel configuration affect
/// file presence. The hierarchy root lacks configured lists and partition state.
/// See [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Cpuset<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Cpuset<'_> {
  /// Reads requested CPU IDs; an empty list means inheritance from ancestors.
  ///
  /// The request stays unchanged by CPU hotplug and can differ from the
  /// effective grant. See [`CpuList`] and [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors); the root lacks this interface.
  pub fn cpus(&self) -> Result<CpuList, Error> { self.cgroup.parse(CpuList::CPUS_FILE_NAME) }

  /// Reads online CPU IDs actually granted to tasks in this cgroup.
  ///
  /// The grant changes with ancestor configuration and CPU hotplug. An empty
  /// effective list means no CPUs are granted, not inheritance. The root also
  /// exposes this file. See [`CpuList`] and [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn cpus_effective(&self) -> Result<CpuList, Error> { self.cgroup.parse(CpuList::EFFECTIVE_FILE_NAME) }

  /// Reads requested NUMA memory-node IDs; an empty list means inheritance.
  ///
  /// The request stays unchanged by memory-node hotplug and can differ from the
  /// effective grant. Reading never migrates memory. See [`MemoryNodeList`] and
  /// [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors); the root lacks this interface.
  pub fn mems(&self) -> Result<MemoryNodeList, Error> { self.cgroup.parse(MemoryNodeList::MEMS_FILE_NAME) }

  /// Reads online memory-node IDs actually granted to tasks in this cgroup.
  ///
  /// The grant changes with ancestor configuration and memory-node hotplug.
  /// An empty effective list means no nodes are granted, not inheritance. The
  /// root also exposes this file. See [`MemoryNodeList`] and
  /// [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn mems_effective(&self) -> Result<MemoryNodeList, Error> {
    self.cgroup.parse(MemoryNodeList::EFFECTIVE_FILE_NAME)
  }

  /// Reads partition membership, isolation, or invalid partition state.
  ///
  /// Hotplug and ancestor configuration can change validity between reads.
  /// Diagnostic reasons are retained when present; older kernels omit them.
  /// See [`CpusetPartition`] and [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
  ///
  /// # Errors
  ///
  /// See the shared [interface-read errors](crate::v2#interface-read-errors);
  /// the root and older kernels lack this interface.
  pub fn partition(&self) -> Result<CpusetPartition, Error> { self.cgroup.parse(CpusetPartition::FILE_NAME) }
}
