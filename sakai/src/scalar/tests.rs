use assertables::assert_ok;
use uom::si::{information::byte, ratio::percent, time::microsecond};

use super::*;
use crate::{
  parse::tests::{
    Cases,
    Failure::{Excess, Invalid, Missing},
  },
  v2::{
    cpu::{CpuIdle, CpuMaxBurst, CpuUclampMax, CpuUclampMin},
    memory::*,
    pids::{PidsCurrent, PidsMax},
  },
};

impl<T: Copy + fmt::Debug + PartialEq + Send + Sync, I: Interface + PartialEq + Send + Sync> Scalar<T, I>
where
  Self: FromStr<Err = ParseError<ParseValueError>>,
{
  fn contract(file: &str, field: &'static str, input: &str, expected: T) {
    assert_eq!(Self::FILE_NAME, file);
    assert_eq!(assert_ok!(input.parse::<Self>()).value(), expected);
    Cases::<Self>::check([("", Err(Missing(field))), ("?", Err(Invalid(field, "?"))), ("1 2", Err(Excess))]);
    assert_eq!(size_of::<Self>(), size_of::<T>());
    assert_eq!(align_of::<Self>(), align_of::<T>());
  }
}

#[test]
fn retains_each_interface_contract() {
  PidsCurrent::contract("pids.current", "current", "123", Count { value: 123, ..Default::default() });
  PidsMax::contract("pids.max", "max", "123", MaxOr::Value(Count { value: 123, ..Default::default() }));
  MemoryOomGroup::contract("memory.oom.group", "oom.group", "1", true);
  MemoryCurrent::contract("memory.current", "current", "123", Bytes::new::<byte>(123));
  MemoryPeak::contract("memory.peak", "peak", "123", Bytes::new::<byte>(123));
  MemoryMax::contract("memory.max", "max", "123", MaxOr::Value(Bytes::new::<byte>(123)));
  MemoryHigh::contract("memory.high", "high", "123", MaxOr::Value(Bytes::new::<byte>(123)));
  MemoryLow::contract("memory.low", "low", "123", Bytes::new::<byte>(123));
  MemoryMin::contract("memory.min", "min", "123", Bytes::new::<byte>(123));
  SwapCurrent::contract("memory.swap.current", "current", "123", Bytes::new::<byte>(123));
  SwapPeak::contract("memory.swap.peak", "peak", "123", Bytes::new::<byte>(123));
  SwapMax::contract("memory.swap.max", "max", "123", MaxOr::Value(Bytes::new::<byte>(123)));
  SwapHigh::contract("memory.swap.high", "high", "123", MaxOr::Value(Bytes::new::<byte>(123)));
  ZswapCurrent::contract("memory.zswap.current", "current", "123", Bytes::new::<byte>(123));
  ZswapMax::contract("memory.zswap.max", "max", "123", MaxOr::Value(Bytes::new::<byte>(123)));
  ZswapWriteback::contract("memory.zswap.writeback", "writeback", "1", true);
  CpuIdle::contract("cpu.idle", "idle", "1", true);
  CpuMaxBurst::contract("cpu.max.burst", "burst", "123", Time::new::<microsecond>(123));
  CpuUclampMin::contract("cpu.uclamp.min", "utilization", "12.34", Ratio::new::<percent>(12.34));
  CpuUclampMax::contract("cpu.uclamp.max", "utilization", "12.34", MaxOr::Value(Ratio::new::<percent>(12.34)));
  let value = Bytes::new::<byte>(123);
  assert_eq!(format!("{:?}", MemoryCurrent::new(value)), format!("MemoryCurrent {{ value: {value:?} }}"));
  const CURRENT: MemoryCurrent = MemoryCurrent::new(Bytes { value: 123, dimension: PhantomData, units: PhantomData });
  const VALUE: Bytes = CURRENT.value();
  assert_eq!(VALUE, value);
}

#[test]
fn parses_scalar_encodings() {
  let count = |value| Count { value, ..Default::default() };
  let cases = [
    ("0\n", Ok(count(0))),
    ("123\n", Ok(count(123))),
    ("18446744073709551615", Ok(count(u64::MAX))),
    ("max", Err(Invalid("current", "max"))),
    ("-1", Err(Invalid("current", "-1"))),
    ("18446744073709551616", Err(Invalid("current", "18446744073709551616"))),
  ];
  Cases::<PidsCurrent>::check(cases.map(|(input, result)| (input, result.map(PidsCurrent::new))));
  Cases::<PidsMax>::check([
    ("max\n", Ok(PidsMax::new(MaxOr::Max))),
    ("0\n", Ok(PidsMax::new(MaxOr::Value(count(0))))),
    ("123\n", Ok(PidsMax::new(MaxOr::Value(count(123))))),
    ("18446744073709551615", Ok(PidsMax::new(MaxOr::Value(count(u64::MAX))))),
    ("-1", Err(Invalid("max", "-1"))),
    ("18446744073709551616", Err(Invalid("max", "18446744073709551616"))),
  ]);
  Cases::<MemoryCurrent>::bytes("current", MemoryCurrent::new);
  Cases::<MemoryMax>::limit("max", MemoryMax::new);
  Cases::<CpuIdle>::boolean("idle", CpuIdle::new);
  Cases::<CpuMaxBurst>::check([
    ("0\n", Ok(CpuMaxBurst::new(Time::new::<microsecond>(0)))),
    ("25000\n", Ok(CpuMaxBurst::new(Time::new::<microsecond>(25_000)))),
    ("18446744073709551", Ok(CpuMaxBurst::new(Time::new::<microsecond>(u64::MAX / 1_000)))),
    ("max", Err(Invalid("burst", "max"))),
    ("-1", Err(Invalid("burst", "-1"))),
    ("18446744073709552", Err(Invalid("burst", "18446744073709552"))),
  ]);
  let cases = [
    ("0.00\n", Ok(Ratio::new::<percent>(0.00))),
    ("12.34\n", Ok(Ratio::new::<percent>(12.34))),
    ("98.76\n", Ok(Ratio::new::<percent>(98.76))),
    ("100.00", Ok(Ratio::new::<percent>(100.00))),
    ("max", Err(Invalid("utilization", "max"))),
    ("unlimited", Err(Invalid("utilization", "unlimited"))),
    ("-0.01", Err(Invalid("utilization", "-0.01"))),
    ("100.01", Err(Invalid("utilization", "100.01"))),
    ("NaN", Err(Invalid("utilization", "NaN"))),
    ("inf", Err(Invalid("utilization", "inf"))),
  ];
  Cases::<CpuUclampMin>::check(cases.map(|(input, result)| (input, result.map(CpuUclampMin::new))));
  Cases::<CpuUclampMax>::check(cases.map(|(input, result)| {
    (input, match input {
      | "max" => Ok(CpuUclampMax::new(MaxOr::Max)),
      | _ => result.map(|value| CpuUclampMax::new(MaxOr::Value(value))),
    })
  }));
}
