//! Accounting for scalar resources exposed by the misc controller.
//!
//! See the [Linux v1 misc controller guide](https://docs.kernel.org/admin-guide/cgroup-v1/misc.html).

use crate::{Count, MaxOr, v2::misc};

v1_file!(alias
  /// Total capacity by resource, available at the hierarchy root.
  MiscCapacity, "misc.capacity" => misc::MiscCapacity
);
v1_file!(alias
  /// Usage by resource in the cgroup and its descendants.
  MiscCurrent, "misc.current" => misc::MiscCurrent
);
v1_file!(alias
  /// Configured limits by resource. Ancestors may impose further restrictions.
  MiscMax, "misc.max" => misc::MiscMax
);
v1_file!(write MiscMax, 'a => (&'a str, MaxOr<Count>), <misc::MiscMax as crate::cgroup::WritableFile>::encode);
