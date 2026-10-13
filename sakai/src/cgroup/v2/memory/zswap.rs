//! Compressed swap accounting and settings in the memory controller.

#[cfg(target_os = "linux")]
use super::super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;
use crate::{
  limit::MaxOr,
  scalar::{Scalar, interface},
  unit::Bytes,
};

/// A borrowed view of one cgroup's compressed swap interfaces.
///
/// Each method reads a fresh snapshot. Kernels without zswap can lack these
/// files, which returns [`crate::Error::FileMissing`].
///
/// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Zswap<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Zswap<'_> {
  /// Memory consumed by the zswap compression backend for this cgroup and descendants.
  ///
  /// Reads a fresh [`ZswapCurrent`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/Documentation/admin-guide/cgroup-v2.rst).
  /// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn current(&self) -> Result<ZswapCurrent, Error> { self.cgroup.parse(ZswapCurrent::FILE_NAME) }

  /// Hard limit on this cgroup's compressed swap pool.
  ///
  /// Reads a fresh [`ZswapMax`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/Documentation/admin-guide/cgroup-v2.rst).
  /// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max(&self) -> Result<ZswapMax, Error> { self.cgroup.parse(ZswapMax::FILE_NAME) }

  /// This cgroup's configured disk swap writeback policy.
  ///
  /// Reads a fresh [`ZswapWriteback`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.8; see the [upstream Linux 6.8 source](https://github.com/torvalds/linux/blob/v6.8/Documentation/admin-guide/cgroup-v2.rst).
  /// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn writeback(&self) -> Result<ZswapWriteback, Error> { self.cgroup.parse(ZswapWriteback::FILE_NAME) }
}

/// Memory consumed by the zswap compression backend for this cgroup and descendants, in bytes.
///
/// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
///
/// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/Documentation/admin-guide/cgroup-v2.rst).
/// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
pub type ZswapCurrent = Scalar<Bytes, interface::ZswapCurrent>;

/// Hard zswap pool limit reported by `memory.zswap.max`.
///
/// `max` means no limit is configured here; ancestor limits can still apply.
///
/// See [Linux `memory.zswap.max` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
///
/// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/Documentation/admin-guide/cgroup-v2.rst).
/// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
pub type ZswapMax = Scalar<MaxOr<Bytes>, interface::ZswapMax>;

/// Disk swap writeback policy reported by `memory.zswap.writeback`.
///
/// Disabling writeback also disables swapping to disk when a zswap store
/// fails, while zswap is enabled globally. This value is the local configured
/// policy. Since Linux 6.11, an ancestor that disables writeback also disables
/// it for children; see the [Linux 6.11 implementation](https://github.com/torvalds/linux/blob/v6.11/mm/memcontrol.c).
///
/// See [Linux `memory.zswap.writeback` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
///
/// Since Linux 6.8; see the [upstream Linux 6.8 source](https://github.com/torvalds/linux/blob/v6.8/Documentation/admin-guide/cgroup-v2.rst).
/// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
pub type ZswapWriteback = Scalar<bool, interface::ZswapWriteback>;
