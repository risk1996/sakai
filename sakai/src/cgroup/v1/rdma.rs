//! Per-device RDMA resource accounting and limits.
//!
//! See the [Linux v1 RDMA controller guide](https://docs.kernel.org/admin-guide/cgroup-v1/rdma.html).

pub use rdma::{RdmaDevice, RdmaDeviceCurrent, RdmaDeviceMax};

use crate::cgroup::common::{interface::cgroup_file, rdma};

cgroup_file!(alias
  /// Current RDMA handle and object usage by device.
  RdmaCurrent, "rdma.current" => rdma::RdmaCurrent
; super::Version);
cgroup_file!(alias
  /// Local RDMA resource limits by device.
  RdmaMax, "rdma.max" => rdma::RdmaMax
; super::Version);
cgroup_file!(write RdmaMax, 'a => (&'a RdmaDevice, &'a RdmaDeviceMax), rdma::encode_device_max);
