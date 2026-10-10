use std::str::FromStr;

use uom::si::{ratio::ratio, time::microsecond};

use crate::{
  error::{ParseError, ParseValueError},
  limit::MaxOr,
  parse::{ParseNonZeroMicroseconds, Parser},
  unit::{NonZeroTime, Ratio},
};

/// The CPU bandwidth limit configured by a cgroup's `cpu.max` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CpuMax {
  quota: MaxOr<NonZeroTime>,
  period: NonZeroTime,
}

impl CpuMax {
  /// The cgroup v2 `cpu.max` interface filename.
  pub const FILE_NAME: &'static str = "cpu.max";

  /// Returns the configured CPU quota.
  #[must_use]
  pub const fn quota(self) -> MaxOr<NonZeroTime> { self.quota }

  /// Returns the quota period.
  #[must_use]
  pub const fn period(self) -> NonZeroTime { self.period }

  /// Returns the CPU bandwidth implied by this cgroup's quota and period.
  ///
  /// An unlimited quota is returned as [`MaxOr::Max`].
  /// This does not account for ancestor limits, affinity, or scheduling policy.
  /// Large quotas may lose precision when converted to a floating-point ratio.
  #[must_use]
  pub fn cpu_count(self) -> MaxOr<Ratio> {
    match self.quota {
      | MaxOr::Max => MaxOr::Max,
      #[expect(clippy::cast_precision_loss, reason = "the public ratio uses f64; large quotas may lose precision")]
      | MaxOr::Value(quota) => {
        let quota = quota.get::<microsecond>() as f64;
        let period = self.period.get::<microsecond>() as f64;

        MaxOr::Value(Ratio::new::<ratio>(quota / period))
      },
    }
  }
}

impl FromStr for CpuMax {
  type Err = ParseError<ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        quota: parser.next_field::<ParseNonZeroMicroseconds, _>("quota")?,
        period: parser.next_field::<ParseNonZeroMicroseconds, _>("period")?,
      })
    })
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_in_delta, assert_ok};
  use uom::si::{ratio::ratio, time::microsecond};

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
}
