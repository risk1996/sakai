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
  /// Memory consumed by this cgroup's zswap compression backend.
  ///
  /// Reads a fresh [`ZswapCurrent`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/Documentation/admin-guide/cgroup-v2.rst).
  /// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::NotSupported`] if the kernel reports an unsupported operation,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Operation`]
  /// for other operating-system failures.
  pub fn current(&self) -> Result<ZswapCurrent, Error> { self.cgroup.parse(ZswapCurrent::FILE_NAME) }

  /// Hard limit on this cgroup's compressed swap pool.
  ///
  /// Reads a fresh [`ZswapMax`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 5.19; see the [upstream Linux 5.19 source](https://github.com/torvalds/linux/blob/v5.19/Documentation/admin-guide/cgroup-v2.rst).
  /// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::NotSupported`] if the kernel reports an unsupported operation,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Operation`]
  /// for other operating-system failures.
  pub fn max(&self) -> Result<ZswapMax, Error> { self.cgroup.parse(ZswapMax::FILE_NAME) }

  /// Whether disk swap writeback is enabled for this cgroup.
  ///
  /// Reads a fresh [`ZswapWriteback`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.8; see the [upstream Linux 6.8 source](https://github.com/torvalds/linux/blob/v6.8/Documentation/admin-guide/cgroup-v2.rst).
  /// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::FileMissing`] if the interface is absent,
  /// [`crate::Error::NotSupported`] if the kernel reports an unsupported operation,
  /// [`crate::Error::Parse`] if its contents are invalid, or [`crate::Error::Operation`]
  /// for other operating-system failures.
  pub fn writeback(&self) -> Result<ZswapWriteback, Error> { self.cgroup.parse(ZswapWriteback::FILE_NAME) }
}

/// Memory consumed by the zswap compression backend in bytes.
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
/// fails. An ancestor that disables writeback also disables it for children.
///
/// See [Linux `memory.zswap.writeback` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
///
/// Since Linux 6.8; see the [upstream Linux 6.8 source](https://github.com/torvalds/linux/blob/v6.8/Documentation/admin-guide/cgroup-v2.rst).
/// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
pub type ZswapWriteback = Scalar<bool, interface::ZswapWriteback>;
