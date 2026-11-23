use std::{fmt::Debug, marker::PhantomData, str::FromStr};

use assertables::{assert_err, assert_ok};
use uom::si::information::byte;

use super::*;
use crate::{
  Pressure,
  error::FieldKind,
  v2::{
    core::{CgroupEvents, CgroupStat, TaskId},
    cpu::{CpuMaxBurst, CpuStat, CpuUclampMax, CpuWeight, Nice},
    cpuset::{CpuList, CpusetPartition},
    dmem::DmemLimits,
    hugetlb::{HugetlbMax, HugetlbNumaStat},
    io::{IoCostModel, IoCostQos, IoLatency, IoMax, IoStat, IoWeight},
    memory::{MemoryEvents, MemoryNumaStat, MemoryStat},
    misc::MiscEvents,
    pids::PidsMax,
    rdma::{RdmaCurrent, RdmaEvents, RdmaMax},
  },
};

#[test]
fn cgroup_codecs_round_trip_representative_snapshots() {
  macro_rules! round_trip {
    ($ty:ty, $input:expr) => {{
      let value = assert_ok!($input.parse::<$ty>());
      let encoded = assert_ok!(value.encode_cgroup());
      assert_eq!(assert_ok!(encoded.parse::<$ty>()), value, "{}", stringify!($ty));
      encoded
    }};
  }

  assert_eq!(round_trip!(CpuMaxBurst, " +025000\n"), "25000");
  assert_eq!(round_trip!(CpuUclampMax, "12.34\n"), "12.34");
  assert_eq!(round_trip!(CpuWeight, "0000\n"), "0");
  assert_eq!(round_trip!(Nice, "+05 "), "5");
  assert_eq!(round_trip!(PidsMax, " max\n"), "max");
  assert_eq!(round_trip!(TaskId, "00042\n"), "42");
  assert_eq!(round_trip!(CgroupEvents, "frozen 0\npopulated 1"), "populated 1\nfrozen 0");
  assert_eq!(round_trip!(CpuList, "00-0002,004-004\n"), "0-2,4");
  assert_eq!(round_trip!(CpusetPartition, "root invalid (no cpu)\n"), "root invalid (no cpu)");
  assert_eq!(round_trip!(HugetlbMax, "max\n"), "max");
  assert_eq!(round_trip!(HugetlbNumaStat, "N1=0 total=4096 N0=4096"), "total=4096 N0=4096 N1=0");
  assert_eq!(round_trip!(DmemLimits, "a max\nb 4096\n"), "a max\nb 4096");
  assert_eq!(round_trip!(MiscEvents, "sev.max 1\n"), "sev.max 1");
  assert_eq!(round_trip!(MemoryStat, "file 2\nanon 1"), "anon 1\nfile 2");
  assert_eq!(round_trip!(MemoryNumaStat, "file N0=5\nanon N2=4 N0=1"), "anon N0=1 N2=4\nfile N0=5");
  assert_eq!(
    round_trip!(CgroupStat, "nr_dying_descendants 0\nnr_descendants 1\nnr_subsys_cpu 1"),
    "nr_descendants 1\nnr_dying_descendants 0\nnr_subsys_cpu 1"
  );
  assert_eq!(
    round_trip!(MemoryEvents, "oom 0\nmax 0\nhigh 0\nlow 0\noom_kill 1"),
    "low 0\nhigh 0\nmax 0\noom 0\noom_kill 1"
  );
  round_trip!(CpuStat, "system_usec 5\nuser_usec 5\nusage_usec 10\nnr_periods -1\nnr_throttled 1\nthrottled_usec 3");
  assert_eq!(round_trip!(IoMax, "8:0 wiops=1 rbps=max riops=2 wbps=max"), "8:0 rbps=max wbps=max riops=2 wiops=1");
  round_trip!(IoLatency, include_str!("../cgroup/v2/fixtures/p3/io.latency"));
  round_trip!(IoWeight, include_str!("../cgroup/v2/fixtures/p3/io.weight"));
  round_trip!(IoStat, include_str!("../cgroup/v2/fixtures/p3/io.stat"));
  round_trip!(IoCostQos, "8:1 enable=1 ctrl=auto rpct=95.00 rlat=123 wpct=90.00 wlat=200 min=50.00 max=150.00");
  round_trip!(IoCostModel, "8:1 ctrl=auto model=linear rbps=1 rseqiops=2 rrandiops=3 wbps=4 wseqiops=5 wrandiops=6");
  round_trip!(Pressure, include_str!("../cgroup/v2/fixtures/p3/io.pressure"));
  round_trip!(RdmaCurrent, include_str!("../cgroup/v2/fixtures/p6/rdma.current"));
  round_trip!(RdmaEvents, include_str!("../cgroup/v2/fixtures/p6/rdma.events"));
  round_trip!(RdmaMax, include_str!("../cgroup/v2/fixtures/p6/rdma.max"));
  assert_eq!(assert_err!(assert_ok!("8:0 future=1".parse::<IoStat>()).encode_cgroup()).field(), "value");
}

/// Parser cases assert values and structured errors without repeating diagnostics.
pub(crate) struct Cases<T>(PhantomData<T>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure<'a> {
  Missing(&'a str),
  Excess,
  Invalid(&'a str, &'a str),
}

impl Failure<'_> {
  fn check(&self, input: &str, error: ParseError<ParseValueError>) {
    let (raw, actual) = match &error {
      | ParseError::Field { raw, field, kind: FieldKind::Missing } => (raw, Failure::Missing(field)),
      | ParseError::Field { raw, field, kind: FieldKind::Excess } => {
        assert_eq!(*field, "additional", "input: {input:?}");
        (raw, Failure::Excess)
      },
      | ParseError::Invalid { raw, field, value, .. } => (raw, Failure::Invalid(field, value)),
    };
    assert_eq!(raw.as_ref(), input);
    assert_eq!(&actual, self, "input: {input:?}: {error}");
  }
}

impl<T> Cases<T>
where
  T: FromStr<Err = ParseError<ParseValueError>> + Debug + PartialEq,
{
  pub(crate) fn check<'a>(cases: impl IntoIterator<Item = (&'a str, Result<T, Failure<'a>>)>) {
    for (input, expected) in cases {
      let actual = input.parse::<T>();
      match expected {
        | Ok(expected) => {
          assert_eq!(assert_ok!(actual), expected, "input: {input:?}")
        },
        | Err(expected) => expected.check(input, assert_err!(actual)),
      }
    }
  }

  pub(crate) fn bytes(field: &'static str, new: impl Fn(Bytes) -> T) {
    Self::check([
      ("0\n", Ok(new(Bytes::new::<byte>(0)))),
      ("1048576", Ok(new(Bytes::new::<byte>(1_048_576)))),
      ("1048576\n", Ok(new(Bytes::new::<byte>(1_048_576)))),
      (" 1048576\n", Ok(new(Bytes::new::<byte>(1_048_576)))),
      ("18446744073709551615", Ok(new(Bytes::new::<byte>(u64::MAX)))),
      ("", Err(Failure::Missing(field))),
      ("1 2", Err(Failure::Excess)),
      ("max", Err(Failure::Invalid(field, "max"))),
      ("-1", Err(Failure::Invalid(field, "-1"))),
      ("18446744073709551616", Err(Failure::Invalid(field, "18446744073709551616"))),
    ]);
  }

  pub(crate) fn limit(field: &'static str, new: impl Fn(MaxOr<Bytes>) -> T) {
    Self::check([
      ("max\n", Ok(new(MaxOr::Max))),
      ("0", Ok(new(MaxOr::Value(Bytes::new::<byte>(0))))),
      ("1048576", Ok(new(MaxOr::Value(Bytes::new::<byte>(1_048_576))))),
      ("1048576\n", Ok(new(MaxOr::Value(Bytes::new::<byte>(1_048_576))))),
      ("18446744073709551615", Ok(new(MaxOr::Value(Bytes::new::<byte>(u64::MAX))))),
      ("", Err(Failure::Missing(field))),
      ("max 1", Err(Failure::Excess)),
      ("-1", Err(Failure::Invalid(field, "-1"))),
      ("18446744073709551616", Err(Failure::Invalid(field, "18446744073709551616"))),
      ("unlimited", Err(Failure::Invalid(field, "unlimited"))),
    ]);
  }

  pub(crate) fn boolean(field: &'static str, new: impl Fn(bool) -> T) {
    Self::check([
      ("0\n", Ok(new(false))),
      ("1", Ok(new(true))),
      ("", Err(Failure::Missing(field))),
      ("1 0", Err(Failure::Excess)),
      ("2", Err(Failure::Invalid(field, "2"))),
      ("-1", Err(Failure::Invalid(field, "-1"))),
      ("true", Err(Failure::Invalid(field, "true"))),
    ]);
  }
}

#[test]
fn displays_parse_errors() {
  for (error, expected) in [
    (ParseError::missing(" \n", "value"), "cgroup content \" \\n\" has missing field \"value\""),
    (ParseError::excess("1 2\n"), "cgroup content \"1 2\\n\" has excess field \"additional\""),
    (
      ParseError::invalid("nope\n", "value", "nope", ParseValueError::OutOfRange),
      "cgroup content \"nope\\n\" has an invalid field \"value\" value \"nope\"",
    ),
  ] {
    assert_eq!(error.to_string(), expected);
  }
}

#[test]
fn rejects_times_that_overflow_nanosecond_storage() {
  for input in ["18446744073709552", "18446744073709551615"] {
    let error =
      assert_err!(Parser::parse(input, |parser| { parser.next_field::<ParseMicroseconds, Time>("duration") }));
    match error {
      | ParseError::Invalid { raw, field, value, source: ParseValueError::OutOfRange } => {
        assert_eq!(raw.as_ref(), input);
        assert_eq!(field, "duration");
        assert_eq!(value.as_ref(), input);
      },
      | other => {
        panic!("expected conversion overflow for {input:?}, got {other:?}")
      },
    }
  }
}
