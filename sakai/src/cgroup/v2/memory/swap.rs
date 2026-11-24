//! Swap accounting and limits in the memory controller.

#[cfg(target_os = "linux")]
use super::super::Cgroup;
#[cfg(target_os = "linux")]
use crate::error::Error;
use crate::{
  limit::MaxOr,
  parse::{ParseCount, cgroup_value},
  scalar::{Scalar, interface},
  unit::{Bytes, Count},
};

/// A borrowed view of one cgroup's swap interfaces.
///
/// Each method reads a fresh snapshot. Root cgroups and kernels without a
/// given interface return [`crate::Error::FileMissing`].
///
/// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct Swap<'a> {
  pub(crate) cgroup: &'a Cgroup,
}

#[cfg(target_os = "linux")]
impl Swap<'_> {
  /// Current swap usage of this cgroup and its descendants, in bytes.
  ///
  /// Reads a fresh [`SwapCurrent`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn current(&self) -> Result<SwapCurrent, Error> { self.cgroup.read_value::<SwapCurrent>() }

  /// Peak swap usage since cgroup creation for this fresh descriptor.
  ///
  /// Reads a fresh [`SwapPeak`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Since Linux 6.5; see the [upstream Linux 6.5 source](https://github.com/torvalds/linux/blob/v6.5/Documentation/admin-guide/cgroup-v2.rst).
  /// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn peak(&self) -> Result<SwapPeak, Error> { self.cgroup.read_value::<SwapPeak>() }

  /// Hard swap limit configured for this cgroup.
  ///
  /// Reads a fresh [`SwapMax`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn max(&self) -> Result<SwapMax, Error> { self.cgroup.read_value::<SwapMax>() }

  /// Sets the swap limit in bytes or removes it with `max`.
  /// See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_max(&self, value: MaxOr<Bytes>) -> Result<(), Error> { self.cgroup.write_value::<SwapMax>(value) }

  /// Swap throttling limit configured for this cgroup.
  ///
  /// Reads a fresh [`SwapHigh`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn high(&self) -> Result<SwapHigh, Error> { self.cgroup.read_value::<SwapHigh>() }

  /// Sets the swap throttling threshold in bytes or removes it with `max`.
  /// See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// # Errors
  /// See the shared [interface-write errors](crate::v2#interface-write-errors).
  pub fn set_high(&self, value: MaxOr<Bytes>) -> Result<(), Error> { self.cgroup.write_value::<SwapHigh>(value) }

  /// Swap high, max, and allocation failure counters.
  ///
  /// Includes descendant events unless the cgroup2 mount uses `memory_localevents`.
  /// With that option, only events originating in this cgroup are reported.
  ///
  /// Reads a fresh [`SwapEvents`] snapshot. See [Linux interface documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  ///
  /// Errors: see the shared [interface-read errors](crate::v2#interface-read-errors).
  pub fn events(&self) -> Result<SwapEvents, Error> { self.cgroup.read_value::<SwapEvents>() }
}

/// Hierarchical swap usage reported by `memory.swap.current`.
///
/// See [Linux `memory.swap.current` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type SwapCurrent = Scalar<Bytes, interface::SwapCurrent>;

/// Peak hierarchical swap usage reported by `memory.swap.peak`.
///
/// Since Linux 6.12, a write resets the peak for that file descriptor.
/// This reader opens a fresh read-only descriptor, so its value is the peak
/// since cgroup creation, in bytes. Older kernels may lack this file. See the
/// [Linux 6.12 reset semantics](https://github.com/torvalds/linux/blob/v6.12/Documentation/admin-guide/cgroup-v2.rst).
///
/// See [Linux `memory.swap.peak` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
///
/// Since Linux 6.5; see the [upstream Linux 6.5 source](https://github.com/torvalds/linux/blob/v6.5/Documentation/admin-guide/cgroup-v2.rst).
/// Older kernels return [`crate::Error::FileMissing`] when this interface is read.
pub type SwapPeak = Scalar<Bytes, interface::SwapPeak>;

/// Hard swap limit reported by `memory.swap.max`.
///
/// `max` means no limit is configured here; ancestor limits can still apply.
///
/// See [Linux `memory.swap.max` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type SwapMax = Scalar<MaxOr<Bytes>, interface::SwapMax>;

/// Swap throttling limit reported by `memory.swap.high`.
///
/// Exceeding this limit throttles further allocations. It is intended for
/// userspace out-of-memory handling rather than routine swap control.
///
/// See [Linux `memory.swap.high` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
pub type SwapHigh = Scalar<MaxOr<Bytes>, interface::SwapHigh>;

/// Swap event counters reported by `memory.swap.events`.
///
/// Counters include events in this cgroup and its descendants unless the cgroup2
/// mount uses `memory_localevents`. With that option, events do not propagate to
/// ancestors, so parent counters exclude descendant failures.
/// The `high` counter is absent on older kernels. Unknown future keys are
/// ignored; known counters must contain valid unsigned event counts.
///
/// See [Linux `memory.swap.events` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
/// The [event propagation implementation](https://github.com/torvalds/linux/blob/v6.12/include/linux/memcontrol.h)
/// applies the mount option to both memory and swap events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SwapEvents {
  high: Option<Count>,
  max: Count,
  fail: Count,
}

crate::v2::interface::cgroup_file!(SwapEvents, "memory.swap.events", Read);

impl SwapEvents {
  /// The cgroup v2 `memory.swap.events` interface filename.
  ///
  /// See [Linux `memory.swap.events` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;

  /// Times swap usage exceeded the high threshold, when reported.
  ///
  /// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  #[must_use]
  pub const fn high(&self) -> Option<Count> { self.high }

  /// Times a swap allocation failed at the max boundary.
  ///
  /// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  #[must_use]
  pub const fn max(&self) -> Count { self.max }

  /// Swap allocation failures from the limit or system-wide exhaustion.
  ///
  /// See [Linux memory interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files).
  #[must_use]
  pub const fn fail(&self) -> Count { self.fail }
}

cgroup_value! { keyed SwapEvents, |key| match key {
      "high" => "high",
      "max" => "max",
      "fail" => "fail",
      _ => "value",
    }, {
  high: optional ParseCount => "high",
  max: required ParseCount => "max",
  fail: required ParseCount => "fail",
} }

#[cfg(test)]
mod tests {
  use indoc::indoc;

  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  };

  #[test]
  fn parses_swap_events() {
    Cases::<SwapEvents>::check([
      (
        indoc! {"
          high 2
          max 3
          fail 4
        "},
        Ok(SwapEvents {
          high: Some(Count { value: 2, ..Default::default() }),
          max: Count { value: 3, ..Default::default() },
          fail: Count { value: 4, ..Default::default() },
        }),
      ),
      (
        "future nope\nfail 0\nmax 1\n",
        Ok(SwapEvents {
          high: None,
          max: Count { value: 1, ..Default::default() },
          fail: Count { value: 0, ..Default::default() },
        }),
      ),
      ("", Err(Missing("max"))),
      ("max 1\n", Err(Missing("fail"))),
      ("max nope\nfail 0", Err(Invalid("max", "nope"))),
      ("max 1\nfail -1", Err(Invalid("fail", "-1"))),
      ("high nope\nmax 1\nfail 0", Err(Invalid("high", "nope"))),
      ("max 1 extra\nfail 0", Err(Excess)),
    ]);
  }
}
