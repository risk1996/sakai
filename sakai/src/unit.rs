use nutype::nutype;
use uom::si::{
  amount_of_substance::mole, electric_current::ampere, length::meter, luminous_intensity::candela, mass::kilogram,
  thermodynamic_temperature::kelvin, time::nanosecond,
};

/// Modified SI dimensions with nanosecond as the base time unit.
///
/// This `dyn` type is only a type-level marker: `uom::Quantity` holds it in
/// `PhantomData`, so no trait object, vtable, or dynamic dispatch is stored
/// or used. Quantities have the size and alignment of their storage type.
pub type BaseUnits = dyn uom::si::Units<
    u64,
    length = meter,
    mass = kilogram,
    time = nanosecond, // default "second"
    electric_current = ampere,
    thermodynamic_temperature = kelvin,
    amount_of_substance = mole,
    luminous_intensity = candela,
  >;

/// Time stored as `u64` nanoseconds (about 584 years of accumulated time).
///
/// CPU time accumulates across CPUs, so this is not a wall-clock uptime limit.
/// Parsing microseconds checks for overflow; direct `uom` constructors and
/// arithmetic remain subject to the storage type's range.
///
/// See [Linux cgroup units and formats](https://docs.kernel.org/admin-guide/cgroup-v2.html#conventions).
pub type Time = uom::si::time::Time<BaseUnits, u64>;
const _: () = {
  assert!(size_of::<Time>() == size_of::<u64>());
  assert!(align_of::<Time>() == align_of::<u64>());
};

/// An amount of memory or other information stored as `u64` bytes.
///
/// See [Linux cgroup units and formats](https://docs.kernel.org/admin-guide/cgroup-v2.html#conventions).
pub type Bytes = uom::si::information::Information<BaseUnits, u64>;
const _: () = {
  assert!(size_of::<Bytes>() == size_of::<u64>());
  assert!(align_of::<Bytes>() == align_of::<u64>());
};

/// An integer bandwidth stored in bytes per second, using second-based units.
///
/// Unlike [`Time`], the base time unit is a second so small bandwidth limits
/// retain their precision. Construct with `uom::si::information_rate::byte_per_second`.
///
/// See [Linux cgroup units and formats](https://docs.kernel.org/admin-guide/cgroup-v2.html#conventions).
pub type BytesPerSecond = uom::si::u64::InformationRate;

/// An integer operation rate stored in operations per second.
///
/// Uses second-based units to preserve small IOPS limits. Construct with
/// `uom::si::frequency::hertz`; one hertz represents one operation per second.
///
/// See [Linux cgroup units and formats](https://docs.kernel.org/admin-guide/cgroup-v2.html#conventions).
pub type OperationsPerSecond = uom::si::u64::Frequency;

/// Separates event counts from other dimensionless quantities.
pub trait CountKind: uom::Kind {}

type Dimensionless<Kind> = uom::si::ISQ<
  uom::typenum::Z0,
  uom::typenum::Z0,
  uom::typenum::Z0,
  uom::typenum::Z0,
  uom::typenum::Z0,
  uom::typenum::Z0,
  uom::typenum::Z0,
  Kind,
>;

/// An event count, with a distinct dimensionless kind and integer storage.
///
/// Construct with `Count { value: 42, ..Default::default() }`.
/// `Count / Time` yields the integer [`EventRate`] and truncates fractions.
/// Use [`NonZeroTime::event_rate`] for ordinary monitoring rates.
///
/// See [Linux cgroup units and formats](https://docs.kernel.org/admin-guide/cgroup-v2.html#conventions).
pub type Count = uom::si::Quantity<Dimensionless<dyn CountKind>, BaseUnits, u64>;
const _: () = {
  assert!(size_of::<Count>() == size_of::<u64>());
  assert!(align_of::<Count>() == align_of::<u64>());
};

/// Separates page quantities from event counts and byte amounts.
pub trait PageKind: uom::Kind {}

/// A number of memory pages, independent of the system's page size.
///
/// Construct with `Pages { value: 42, ..Default::default() }`.
///
/// To convert to bytes, obtain the system page size; see [sysconf(3)](https://man7.org/linux/man-pages/man3/sysconf.3.html).
pub type Pages = uom::si::Quantity<Dimensionless<dyn PageKind>, BaseUnits, u64>;
const _: () = {
  assert!(size_of::<Pages>() == size_of::<u64>());
  assert!(align_of::<Pages>() == align_of::<u64>());
};

/// An integer event rate stored in events per nanosecond.
///
/// `Count / Time` truncates before converting units: 100 events over one second
/// becomes zero. Use [`NonZeroTime::event_rate`] to retain fractional rates.
/// Direct division by a zero [`Time`] panics, following integer arithmetic.
///
/// See [Linux cgroup units and formats](https://docs.kernel.org/admin-guide/cgroup-v2.html#conventions).
pub type EventRate = uom::si::frequency::Frequency<BaseUnits, u64>;
const _: () = {
  assert!(size_of::<EventRate>() == size_of::<u64>());
  assert!(align_of::<EventRate>() == align_of::<u64>());
};

/// A fractional event rate stored in events per second.
///
/// Uses `uom`'s floating-point SI frequency and supports conversion with
/// `uom::si::frequency::hertz`. Calculate with [`NonZeroTime::event_rate`].
pub type FractionalEventRate = uom::si::f64::Frequency;

/// A nonzero [`Time`].
///
/// Use `try_new` to reject a zero duration and `into_inner` to recover the quantity.
/// For example, [`crate::v2::cpu::CpuMax`] requires a nonzero quota and period.
///
/// See [Linux CPU bandwidth units](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpu-interface-files).
#[nutype(
  validate(predicate = |time| time.get::<nanosecond>() != 0),
  derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRef),
)]
pub struct NonZeroTime(Time);
const _: () = {
  assert!(size_of::<NonZeroTime>() == size_of::<Time>());
  assert!(align_of::<NonZeroTime>() == align_of::<Time>());
};

impl NonZeroTime {
  /// Calculates an event rate over this nonzero elapsed duration.
  ///
  /// The result retains fractional events per second. Converting integer
  /// counts and nanoseconds to `f64` can round values above 2^53; every valid
  /// input produces a finite, nonnegative rate. Zero counts produce zero.
  /// `try_new` rejects zero elapsed time before calculation. Supply an already
  /// computed count delta; this method does not handle counter resets or rollover.
  ///
  /// ```
  /// use sakai::{Count, NonZeroTime, Time};
  /// use uom::si::{frequency::hertz, time::second};
  ///
  /// let elapsed = NonZeroTime::try_new(Time::new::<second>(2))?;
  /// let count = Count { value: 1, ..Default::default() };
  /// assert_eq!(elapsed.event_rate(count).get::<hertz>(), 0.5);
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  #[expect(clippy::cast_precision_loss, reason = "fractional rates use f64; rounding above 2^53 is documented")]
  pub fn event_rate(&self, count: Count) -> FractionalEventRate {
    let elapsed = uom::si::f64::Time::new::<nanosecond>(self.as_ref().get::<nanosecond>() as f64);
    FractionalEventRate::new::<uom::si::frequency::hertz>(count.value as f64 / elapsed.get::<uom::si::time::second>())
  }
}

/// A dimensionless ratio stored as an `f64` to preserve fractional values.
/// Standard SI integer ratios normalize to whole units and truncate percents.
///
/// See [Linux cgroup units and formats](https://docs.kernel.org/admin-guide/cgroup-v2.html#conventions).
pub type Ratio = uom::si::f64::Ratio;

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_in_delta, assert_ok};
  use uom::si::time::{microsecond, millisecond, nanosecond, second};

  use super::*;
  use crate::parse::{ParseCgroup, ParsePercent};

  #[test]
  fn count_has_dimensionally_typed_rates() {
    let count = Count { value: 2_000_000_000, ..Default::default() };
    let rate: EventRate = count / Time::new::<second>(1);
    assert_eq!(rate.get::<uom::si::frequency::hertz>(), 2_000_000_000);
  }

  #[test]
  fn fractional_event_rates_preserve_monitoring_values_and_boundaries() {
    for (count, nanos, expected) in [
      (100, 1_000_000_000, 100.0),
      (1, 2_000_000_000, 0.5),
      (0, 1, 0.0),
      (1, 1, 1_000_000_000.0),
      (u64::MAX, u64::MAX, 1_000_000_000.0),
    ] {
      let elapsed = assert_ok!(NonZeroTime::try_new(Time::new::<nanosecond>(nanos)));
      let rate = elapsed.event_rate(Count { value: count, ..Default::default() });
      assert_in_delta!(rate.get::<uom::si::frequency::hertz>(), expected, expected * 1e-15 + f64::EPSILON);
    }
    let elapsed = assert_ok!(NonZeroTime::try_new(Time::new::<nanosecond>(u64::MAX)));
    assert!(elapsed.event_rate(Count { value: 1, ..Default::default() }).value > 0.0);
    let shortest = assert_ok!(NonZeroTime::try_new(Time::new::<nanosecond>(1)));
    assert!(shortest.event_rate(Count { value: u64::MAX, ..Default::default() }).value.is_finite());
    assert_err!(NonZeroTime::try_new(Time::new::<nanosecond>(0)));
  }

  #[test]
  fn fractional_ratios_preserve_kernel_percentages() {
    for (input, expected) in [("12.34", 0.1234), ("0.01", 0.0001), ("100.00", 1.0)] {
      let parsed = assert_ok!(<Ratio as ParseCgroup<ParsePercent>>::parse_cgroup(input));
      assert_in_delta!(parsed.get::<uom::si::ratio::ratio>(), expected, f64::EPSILON);
    }
  }

  #[test]
  fn preserves_representable_microseconds_as_nanoseconds() {
    for value in [0, 1, 25_000, 100_000, 1_500_001, u64::MAX / 1_000] {
      let time = Time::new::<microsecond>(value);
      assert_eq!(time.get::<microsecond>(), value);
      assert_eq!(time.get::<nanosecond>(), value * 1_000);
    }
  }

  #[test]
  fn converts_u64_max_nanoseconds_to_coarser_units() {
    let time = Time::new::<nanosecond>(u64::MAX);
    assert_eq!(time.get::<second>(), 18_446_744_073);
    assert_eq!(time.get::<millisecond>(), 18_446_744_073_709);
    assert_eq!(time.get::<microsecond>(), 18_446_744_073_709_551);
  }
}
