use uom::si::{ratio::ratio, time::nanosecond};

use crate::{
  limit::MaxOr,
  parse::{ParseNonZeroMicroseconds, cgroup_value},
  unit::{NonZeroTime, Ratio},
  v2::interface::cgroup_file,
};

/// The CPU bandwidth limit configured by a cgroup's `cpu.max` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value.
/// The first token is the quota, or `max`, and the second is its period;
/// both numeric tokens are microseconds. The fair-class scheduler enforces
/// this limit. A BPF scheduler can also apply it through its bandwidth callback.
///
/// See [Linux `cpu.max` interface](https://www.kernel.org/doc/html/v7.3/admin-guide/cgroup-v2.html#cpu-interface-files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct CpuMax {
  /// CPU time allowed per period, or `max` for no quota at this cgroup.
  pub quota: MaxOr<NonZeroTime>,
  /// Length of the CPU bandwidth enforcement period.
  pub period: NonZeroTime,
}

cgroup_value! {
  positional CpuMax {
    quota: ParseNonZeroMicroseconds,
    period: ParseNonZeroMicroseconds,
  }
}
cgroup_file!(CpuMax, "cpu.max", ReadWrite);

impl CpuMax {
  /// The cgroup v2 `cpu.max` interface filename.
  ///
  /// See [Linux `cpu.max` interface](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  pub const FILE_NAME: &'static str = <Self as crate::v2::CgroupFile>::FILE_NAME;

  /// Returns the CPU bandwidth implied by this cgroup's quota and period.
  ///
  /// An unlimited quota is returned as [`MaxOr::Max`].
  /// This does not account for ancestor limits, affinity, or scheduling policy.
  /// Large quotas may lose precision when converted to a floating-point ratio.
  /// For example, a quota of 25,000 microseconds over a 100,000-microsecond
  /// period gives 0.25 CPUs, exposed as a dimensionless [`Ratio`].
  ///
  /// See [Linux CPU interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
  #[must_use]
  pub fn cpu_count(&self) -> MaxOr<Ratio> {
    match self.quota {
      | MaxOr::Max => MaxOr::Max,
      #[expect(clippy::cast_precision_loss, reason = "the public ratio uses f64; large quotas may lose precision")]
      | MaxOr::Value(quota) => {
        let quota = quota.as_ref().get::<nanosecond>() as f64;
        let period = self.period.as_ref().get::<nanosecond>() as f64;

        MaxOr::Value(Ratio::new::<ratio>(quota / period))
      },
    }
  }
}

/// Quota-only command for `cpu.max`. The kernel keeps the existing period.
#[non_exhaustive]
pub struct CpuQuota;

cgroup_file!(file CpuQuota, CpuMax::FILE_NAME, Write);
cgroup_file!(write CpuQuota, 'a => MaxOr<NonZeroTime>, |quota| {
  let mut command = String::new();
  crate::parse::EncodeCgroup::<ParseNonZeroMicroseconds>::encode_field(&quota, &mut command, "quota")?;
  Ok(command)
});

#[cfg(test)]
mod tests {
  use assertables::{assert_in_delta, assert_ok};
  use uom::si::{
    ratio::ratio,
    time::{microsecond, nanosecond},
  };

  use super::*;
  use crate::{
    parse::tests::{
      Cases,
      Failure::{Excess, Invalid, Missing},
    },
    unit::Time,
  };

  #[test]
  fn parses_cpu_max() {
    let ms_25_000 = assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(25_000)));
    let ms_100_000 = assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(100_000)));
    let ms_1_500_001 = assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(1_500_001)));
    let ms_max = assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(u64::MAX / 1_000,)));
    Cases::<CpuMax>::check([
      ("25000 100000\n", Ok(CpuMax { quota: MaxOr::Value(ms_25_000), period: ms_100_000 })),
      ("max 100000\n", Ok(CpuMax { quota: MaxOr::Max, period: ms_100_000 })),
      ("1500001 100000\n", Ok(CpuMax { quota: MaxOr::Value(ms_1_500_001), period: ms_100_000 })),
      ("18446744073709551 100000\n", Ok(CpuMax { quota: MaxOr::Value(ms_max), period: ms_100_000 })),
      ("", Err(Missing("quota"))),
      ("max", Err(Missing("period"))),
      ("max 100000 extra", Err(Excess)),
      ("unlimited 100000", Err(Invalid("quota", "unlimited"))),
      ("max forever", Err(Invalid("period", "forever"))),
      ("max max", Err(Invalid("period", "max"))),
      ("25000 0", Err(Invalid("period", "0"))),
      ("0 100000", Err(Invalid("quota", "0"))),
      (" max forever\n", Err(Invalid("period", "forever"))),
    ]);
  }

  #[test]
  fn encodes_cpu_max_in_kernel_units_and_field_order() {
    for (input, expected) in [
      (" \tmax\t100000\r\n", "max 100000"),
      ("025000  +100000\n", "25000 100000"),
      ("18446744073709551 1000", "18446744073709551 1000"),
    ] {
      let value = assert_ok!(input.parse::<CpuMax>());
      let encoded = assert_ok!(value.encode_cgroup());
      assert_eq!(encoded, expected);
      assert_eq!(assert_ok!(encoded.parse::<CpuMax>()), value);
    }
  }

  #[test]
  fn rejects_sub_microsecond_remainders_in_both_fields() {
    let exact = assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(1_000)));
    let inexact = assert_ok!(NonZeroTime::try_new(Time::new::<nanosecond>(1_000_001)));
    for (value, field) in [
      (CpuMax { quota: MaxOr::Value(inexact), period: exact }, "quota"),
      (CpuMax { quota: MaxOr::Max, period: inexact }, "period"),
    ] {
      assert_eq!(value.encode_cgroup().map_err(|error| error.field()), Err(field));
    }
  }

  #[test]
  fn calculates_cpu_count() {
    let cases = [
      ("25000 100000", 0.25),
      ("100000 100000", 1.0),
      ("1500001 100000", 15.000_01),
      ("1 3", 1.0 / 3.0),
      ("17592186044415 1000", 17_592_186_044.415),
    ];

    for (input, expected) in cases {
      let cpu_max = assert_ok!(input.parse::<CpuMax>());
      let cpu_count = cpu_max.cpu_count();
      assert!(matches!(cpu_count, MaxOr::Value(_)));

      if let MaxOr::Value(cpu_count) = cpu_count {
        assert_in_delta!(cpu_count.get::<ratio>(), expected, f64::EPSILON);
      }
    }
  }

  #[test]
  fn preserves_unlimited_cpu_count() {
    let cpu_max = assert_ok!("max 100000".parse::<CpuMax>());

    assert!(matches!(cpu_max.cpu_count(), MaxOr::Max));
  }

  #[test]
  fn calculates_cpu_count_for_directly_constructed_nanosecond_durations() {
    let limit = CpuMax {
      quota: MaxOr::Value(assert_ok!(NonZeroTime::try_new(Time::new::<nanosecond>(250)))),
      period: assert_ok!(NonZeroTime::try_new(Time::new::<nanosecond>(500))),
    };
    let MaxOr::Value(count) = limit.cpu_count() else { panic!("expected finite CPU quota") };
    assert_in_delta!(count.get::<ratio>(), 0.5, f64::EPSILON);
  }
}
