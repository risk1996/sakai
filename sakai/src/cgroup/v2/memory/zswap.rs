//! Compressed swap accounting and settings in the memory controller.

use crate::{
  limit::MaxOr,
  scalar::{Scalar, interface},
  unit::Bytes,
};

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
