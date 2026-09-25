use nutype::nutype;
use uom::si::{
  amount_of_substance::mole, electric_current::ampere, length::meter,
  luminous_intensity::candela, mass::kilogram,
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
pub type Time = uom::si::time::Time<BaseUnits, u64>;
const _: () = {
  assert!(size_of::<Time>() == size_of::<u64>());
  assert!(align_of::<Time>() == align_of::<u64>());
};

/// Separates event counts from other dimensionless quantities.
pub trait CountKind: uom::Kind {}

/// An event count, with a distinct dimensionless kind and integer storage.
///
/// Construct with `Count { value: 42, ..Default::default() }`.
/// `Count / Time` yields [`EventRate`]. Integer arithmetic truncates: convert
/// operands to fractional storage before computing sub-unit rates.
pub type Count = uom::si::Quantity<
  uom::si::ISQ<
    uom::typenum::Z0,
    uom::typenum::Z0,
    uom::typenum::Z0,
    uom::typenum::Z0,
    uom::typenum::Z0,
    uom::typenum::Z0,
    uom::typenum::Z0,
    dyn CountKind,
  >,
  BaseUnits,
  u64,
>;

/// An integer event rate stored in events per nanosecond.
pub type EventRate = uom::si::frequency::Frequency<BaseUnits, u64>;
const _: () = {
  assert!(size_of::<Count>() == size_of::<u64>());
  assert!(align_of::<Count>() == align_of::<u64>());
};

/// A nonzero [`Time`].
#[nutype(
  validate(predicate = |time| time.get::<nanosecond>() != 0),
  derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRef, Deref),
)]
pub struct NonZeroTime(Time);
const _: () = {
  assert!(size_of::<NonZeroTime>() == size_of::<Time>());
  assert!(align_of::<NonZeroTime>() == align_of::<Time>());
};

/// A dimensionless ratio stored as an `f64` to preserve fractional values.
/// Standard SI integer ratios normalize to whole units and truncate percents.
pub type Ratio = uom::si::f64::Ratio;

#[cfg(test)]
mod tests {
  use uom::si::time::{microsecond, millisecond, nanosecond, second};

  use super::*;

  #[test]
  fn count_has_dimensionally_typed_rates() {
    let count = Count {
      value: 2_000_000_000,
      ..Default::default()
    };
    let rate: EventRate = count / Time::new::<second>(1);
    assert_eq!(rate.get::<uom::si::frequency::hertz>(), 2_000_000_000);
  }

  #[test]
  fn fractional_ratios_preserve_kernel_percentages() {
    use assertables::{assert_in_delta, assert_ok};

    use crate::cgroup::common::parser::{ParseCgroup, ParsePercent};
    for (input, expected) in
      [("12.34", 0.1234), ("0.01", 0.0001), ("100.00", 1.0)]
    {
      let parsed =
        assert_ok!(<Ratio as ParseCgroup<ParsePercent>>::parse_cgroup(input));
      assert_in_delta!(
        parsed.get::<uom::si::ratio::ratio>(),
        expected,
        f64::EPSILON
      );
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
  fn preserves_submicrosecond_precision() {
    for value in [0, 1, 999, 1_001, u64::MAX] {
      assert_eq!(Time::new::<nanosecond>(value).get::<nanosecond>(), value);
    }
  }

  #[test]
  fn converts_units_and_supports_quantity_arithmetic() {
    let time = Time::new::<microsecond>(25_000);
    assert_eq!(time.get::<nanosecond>(), 25_000_000);
    assert_eq!(time.get::<millisecond>(), 25);
    assert_eq!(Time::new::<second>(1).get::<microsecond>(), 1_000_000);
    assert_eq!((time + time).get::<microsecond>(), 50_000);
  }

  #[test]
  fn converts_u64_max_nanoseconds_to_coarser_units() {
    let time = Time::new::<nanosecond>(u64::MAX);
    assert_eq!(time.get::<second>(), 18_446_744_073);
    assert_eq!(time.get::<millisecond>(), 18_446_744_073_709);
    assert_eq!(time.get::<microsecond>(), 18_446_744_073_709_551);
  }
}
