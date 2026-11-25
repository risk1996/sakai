//! Accounting for scalar resources exposed by the misc controller.
//!
//! See the [Linux v1 misc controller guide](https://docs.kernel.org/admin-guide/cgroup-v1/misc.html).

use crate::{
  Count, MaxOr,
  cgroup::common::{interface::cgroup_file, misc},
};

cgroup_file!(alias
  /// Total capacity by resource, available at the hierarchy root.
  MiscCapacity, "misc.capacity" => misc::MiscCapacity
; super::Version);
cgroup_file!(alias
  /// Usage by resource in the cgroup and its descendants.
  MiscCurrent, "misc.current" => misc::MiscCurrent
; super::Version);
cgroup_file!(alias
  /// Configured limits by resource. Ancestors may impose further restrictions.
  MiscMax, "misc.max" => misc::MiscMax
; super::Version);
cgroup_file!(write MiscMax, 'a => (&'a str, MaxOr<Count>), misc::encode_limit);
