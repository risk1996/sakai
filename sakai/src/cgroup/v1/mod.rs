//! Typed interfaces for legacy Linux cgroup hierarchies.
//!
//! Select a controller or named hierarchy when opening a process's cgroup.
//! Controllers on the same hierarchy share a directory. A hybrid system may
//! also expose a separate handle from [crate::v2] for the same process.
//! Snapshots can be parsed on any OS. File operations require Linux.
//! See the [Linux cgroup v1 guide](https://docs.kernel.org/admin-guide/cgroup-v1/index.html).
//!
//! File declarations are version-specific.
//!
//! ```compile_fail,E0271
//! use sakai::{cgroup::ReadableFile, v1::Version, v2::cpu::CpuMax};
//! fn v1_file<F: ReadableFile<Version = Version>>() {}
//! v1_file::<CpuMax>();
//! ```

/// Type parameter selecting cgroup v1 file declarations.
#[derive(Debug)]
#[non_exhaustive]
pub struct Version;

#[cfg(target_os = "linux")]
pub use handle::Cgroup;

pub use super::{CgroupFile, ReadableFile, WritableFile};

#[cfg(target_os = "linux")]
mod handle;

macro_rules! v1_file {
  (read $($declaration:tt)*) => {
    $crate::cgroup::interface::cgroup_file!(read $($declaration)*);
  };
  (write $($declaration:tt)*) => {
    $crate::cgroup::interface::cgroup_file!(write $($declaration)*);
  };
  ($($declaration:tt)*) => {
    $crate::cgroup::interface::cgroup_file!($($declaration)*; $crate::v1::Version);
  };
}

macro_rules! v1_scalar_snapshot {
  ($(#[$doc:meta])* $name:ident, $value:ty, $codec:ty, $field:literal) => {
    $(#[$doc])*
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub struct $name {
      /// Value reported by the kernel.
      pub value: $value,
    }

    $crate::parse::cgroup_value!(scalar $name, value: $codec, $field);
  };
}

macro_rules! v1_scalars {
  ($($(#[$doc:meta])* $name:ident, $file:literal, $value:ty, $codec:ty, $access:ident;)+) => {
    $(
      v1_scalar_snapshot!($(#[$doc])* $name, $value, $codec, $file);
      v1_file!($name, $file, Read);
      v1_scalars!(@write $name, $value, $access);
    )+
  };
  (@write $name:ident, $value:ty, Read) => {};
  (@write $name:ident, $value:ty, ReadWrite) => {
    v1_file!(write $name, 'a => $value, |value| $name { value }.encode_cgroup());
  };
}

pub mod blkio;
pub mod core;
pub mod cpu;
pub mod cpuacct;
pub mod cpuset;
pub mod devices;
pub mod freezer;
pub mod hugetlb;
pub mod memory;
pub mod misc;
pub mod net_cls;
pub mod net_prio;
pub mod pids;
pub mod rdma;

mod codec;
