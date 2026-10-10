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
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Zswap<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Zswap<'_> {
  /// Memory consumed by this cgroup's zswap compression backend.
  pub fn current(&self) -> Result<ZswapCurrent, Error> { self.cgroup.parse(ZswapCurrent::FILE_NAME) }

  /// Hard limit on this cgroup's compressed swap pool.
  pub fn max(&self) -> Result<ZswapMax, Error> { self.cgroup.parse(ZswapMax::FILE_NAME) }

  /// Whether disk swap writeback is enabled for this cgroup.
  pub fn writeback(&self) -> Result<ZswapWriteback, Error> { self.cgroup.parse(ZswapWriteback::FILE_NAME) }
}

/// Memory consumed by the zswap compression backend in bytes.
pub type ZswapCurrent = Scalar<Bytes, interface::ZswapCurrent>;

/// Hard zswap pool limit reported by `memory.zswap.max`.
///
/// `max` means no limit is configured here; ancestor limits can still apply.
pub type ZswapMax = Scalar<MaxOr<Bytes>, interface::ZswapMax>;

/// Disk swap writeback policy reported by `memory.zswap.writeback`.
///
/// Disabling writeback also disables swapping to disk when a zswap store
/// fails. An ancestor that disables writeback also disables it for children.
pub type ZswapWriteback = Scalar<bool, interface::ZswapWriteback>;
