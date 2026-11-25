//! Fault and reservation accounting for each supported huge page size.
//!
//! See the [Linux v1 HugeTLB guide](https://docs.kernel.org/admin-guide/cgroup-v1/hugetlb.html).

use super::codec::encode_byte_limit;
pub use crate::v2::hugetlb::PageSize;
use crate::{
  Bytes, Count, MaxOr,
  parse::{ParseBytes, ParseCount},
};

macro_rules! hugetlb_files {
  ($($(#[$doc:meta])* $name:ident, $suffix:literal, $value:ty, $codec:ty;)+) => {
    $(
      v1_scalar_snapshot!($(#[$doc])* $name, $value, $codec, $suffix);
      v1_file!(file $name, concat!("hugetlb.<hugepagesize>.", $suffix),
        PageSize => |size: &PageSize| size.file_name($suffix));
      v1_file!(read $name => $name);
    )+
  };
}

hugetlb_files! {
  /// Faulted HugeTLB usage in bytes, including descendants.
  HugetlbUsage, "usage_in_bytes", Bytes, ParseBytes;
  /// Local HugeTLB fault limit in bytes, including the kernel's numeric unlimited sentinel.
  HugetlbLimit, "limit_in_bytes", Bytes, ParseBytes;
  /// Peak faulted HugeTLB usage in bytes.
  HugetlbPeak, "max_usage_in_bytes", Bytes, ParseBytes;
  /// Count of failed allocations due to the HugeTLB fault limit.
  HugetlbFailures, "failcnt", Count, ParseCount;
  /// Bytes charged for reservations and faults without a reservation, including descendants.
  HugetlbReservedUsage, "rsvd.usage_in_bytes", Bytes, ParseBytes;
  /// Local reservation-accounting limit in bytes, including the kernel's numeric unlimited sentinel.
  HugetlbReservedLimit, "rsvd.limit_in_bytes", Bytes, ParseBytes;
  /// Peak reservation-accounting usage in bytes.
  HugetlbReservedPeak, "rsvd.max_usage_in_bytes", Bytes, ParseBytes;
  /// Count of failed allocations due to the reservation-accounting limit.
  HugetlbReservedFailures, "rsvd.failcnt", Count, ParseCount;
}

v1_file!(write HugetlbLimit, 'a => MaxOr<Bytes>, |value| encode_byte_limit(value, "limit_in_bytes"));
v1_file!(write HugetlbReservedLimit, 'a => MaxOr<Bytes>, |value| encode_byte_limit(value, "rsvd.limit_in_bytes"));
