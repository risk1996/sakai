//! Per-device RDMA resource accounting and limits.
//!
//! See the [Linux v1 RDMA controller guide](https://docs.kernel.org/admin-guide/cgroup-v1/rdma.html).

pub use rdma::{RdmaDevice, RdmaDeviceCurrent, RdmaDeviceMax};

use crate::v2::rdma;

v1_file!(alias
  /// Current RDMA handle and object usage by device.
  RdmaCurrent, "rdma.current" => rdma::RdmaCurrent
);
v1_file!(alias
  /// Local RDMA resource limits by device.
  RdmaMax, "rdma.max" => rdma::RdmaMax
);
v1_file!(write RdmaMax, 'a => (&'a RdmaDevice, &'a RdmaDeviceMax), <rdma::RdmaMax as crate::cgroup::WritableFile>::encode);
