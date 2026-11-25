use std::{fmt, str::FromStr};

use nutype::nutype;
use uom::si::information::byte;

use crate::{
  Bytes,
  error::{ParseError, ParseValueError},
  parse::cgroup_value,
};

/// A HugeTLB page size expressed in bytes, independent of the system page size.
///
/// `try_new` accepts powers of two of at least 1 KiB; this validates the name,
/// not whether the running kernel supports that size. Parse kernel labels such
/// as `64KB`, `2MB`, or `1GB`, whose suffixes denote binary units. Labels must
/// use the kernel's canonical unit and decimal spelling. `AsRef` and
/// `into_inner` expose the byte quantity.
///
/// ```
/// use sakai::cgroup::common::PageSize;
/// use uom::si::information::byte;
///
/// let size = "2MB".parse::<PageSize>()?;
/// assert_eq!(size.as_ref().get::<byte>(), 2_097_152);
/// assert_eq!(size.to_string(), "2MB");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// See [Linux HugeTLB filenames](https://github.com/torvalds/linux/blob/v5.15/mm/hugetlb_cgroup.c).
#[nutype(
  validate(predicate = |size: &Bytes| {
    let bytes = size.get::<byte>();
    bytes >= PageSize::KIBIBYTE && bytes.is_power_of_two()
  }),
  derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, AsRef)
)]
#[non_exhaustive]
pub struct PageSize(Bytes);

cgroup_value! { display PageSize }

impl PageSize {
  const GIBIBYTE: u64 = 1 << 30;
  const KIBIBYTE: u64 = 1 << 10;
  const MEBIBYTE: u64 = 1 << 20;

  pub(in crate::cgroup) fn file_name(&self, suffix: &str) -> String { format!("hugetlb.{self}.{suffix}") }
}

impl fmt::Display for PageSize {
  #[expect(clippy::arithmetic_side_effects, reason = "selected binary unit divisors are positive constants")]
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let bytes = self.as_ref().get::<byte>();
    let (unit, suffix) = if bytes >= Self::GIBIBYTE {
      (Self::GIBIBYTE, "GB")
    } else if bytes >= Self::MEBIBYTE {
      (Self::MEBIBYTE, "MB")
    } else {
      (Self::KIBIBYTE, "KB")
    };
    write!(formatter, "{}{suffix}", bytes / unit)
  }
}

impl FromStr for PageSize {
  type Err = ParseError<ParseValueError>;

  fn from_str(label: &str) -> Result<Self, Self::Err> {
    let invalid = || ParseError::invalid(label, "page_size", label, ParseValueError::OutOfRange);
    let (number, unit) = if let Some(number) = label.strip_suffix("GB") {
      (number, Self::GIBIBYTE)
    } else if let Some(number) = label.strip_suffix("MB") {
      (number, Self::MEBIBYTE)
    } else if let Some(number) = label.strip_suffix("KB") {
      (number, Self::KIBIBYTE)
    } else {
      return Err(invalid());
    };
    let bytes = number.parse::<u64>().ok().and_then(|number| number.checked_mul(unit)).ok_or_else(invalid)?;
    let size = Self::try_new(Bytes::new::<byte>(bytes)).map_err(|_error| invalid())?;
    if size.to_string() != label {
      return Err(invalid());
    }
    Ok(size)
  }
}
