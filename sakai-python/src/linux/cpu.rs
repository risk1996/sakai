//! Cpu bindings and conversions.

use std::sync::Arc;

use pyo3::prelude::*;
use sakai::v2::cpu::{
  CpuBandwidthStat as CoreCpuBandwidthStat, CpuBurstStat as CoreCpuBurstStat, CpuMax as CoreCpuMax,
  CpuStat as CoreCpuStat, CpuStatLocal as CoreCpuStatLocal, CpuTimeStat as CoreCpuTimeStat, CpuWeight as CoreCpuWeight,
};

use super::{MaxOr, Pressure, PythonError};
use crate::quantity::Quantity;

/// Fresh reads of CPU controller files for one pinned cgroup.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, module = "sakai._sakai")]
pub(super) struct CpuReader {
  pub(super) owner: Arc<sakai::Cgroup>,
}

#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubs"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl CpuReader {
  /// Read hierarchical CPU usage and optional bandwidth counters.
  fn stat(&self, py: Python<'_>) -> PyResult<CpuStat> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().stat()).map(Into::into)
  }

  /// Read local CPU throttling counters, if the interface exists.
  fn stat_local(&self, py: Python<'_>) -> PyResult<CpuStatLocal> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().stat_local()).map(Into::into)
  }

  /// Read this cgroup's quota and period, not effective ancestor capacity.
  fn max(&self, py: Python<'_>) -> PyResult<CpuMax> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().max()).map(Into::into)
  }

  /// Read CPU weight, preserving the special idle state.
  fn weight(&self, py: Python<'_>) -> PyResult<CpuWeight> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().weight()).map(Into::into)
  }

  /// Read CPU weight as a nice value from -20 through 19.
  fn weight_nice(&self, py: Python<'_>) -> PyResult<i8> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().weight_nice()).map(|nice| *nice.as_ref())
  }

  /// Read the allowed CPU bandwidth burst in nanoseconds.
  fn max_burst(&self, py: Python<'_>) -> PyResult<Quantity<sakai::Time>> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().max_burst()).map(|burst| burst.value().into())
  }

  /// Read whether this cgroup is configured for idle CPU scheduling.
  fn idle(&self, py: Python<'_>) -> PyResult<bool> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().idle()).map(|idle| idle.value())
  }

  /// Read the minimum utilization clamp as a ratio from zero to one.
  fn uclamp_min(&self, py: Python<'_>) -> PyResult<Quantity<sakai::Ratio>> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().uclamp_min()).map(|clamp| clamp.value().into())
  }

  /// Read the maximum utilization clamp as `max` or a concrete ratio.
  #[gen_stub(override_return_type(type_repr = "MaxOr[pint.Quantity[float]]"))]
  fn uclamp_max(&self, py: Python<'_>) -> PyResult<MaxOr> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().uclamp_max()).map(|clamp| MaxOr::from_ratio(clamp.value()))
  }

  /// Read CPU pressure-stall averages and cumulative times.
  fn pressure(&self, py: Python<'_>) -> PyResult<Pressure> {
    PythonError::read(py, &self.owner, |owner| owner.cpu().pressure()).map(Into::into)
  }
}

/// Hierarchical CPU usage and optional bandwidth statistics.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CpuStat {
  /// CPU usage split into total, user, and system nanoseconds.
  time: CpuTimeStat,
  /// Bandwidth counters, or None when the kernel omitted them.
  bandwidth: Option<CpuBandwidthStat>,
}

impl From<CoreCpuStat> for CpuStat {
  fn from(value: CoreCpuStat) -> Self {
    Self { time: value.time().into(), bandwidth: value.bandwidth().map(Into::into) }
  }
}

/// Local CPU statistics, distinct from hierarchical cpu.stat.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CpuStatLocal {
  /// Locally throttled time in nanoseconds, if reported by the kernel.
  throttled: Option<Quantity<sakai::Time>>,
}

impl From<CoreCpuStatLocal> for CpuStatLocal {
  fn from(value: CoreCpuStatLocal) -> Self { Self { throttled: value.throttled().map(|time| time.into()) } }
}

/// CPU usage times in nanoseconds.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CpuTimeStat {
  /// Total CPU usage in nanoseconds.
  usage: Quantity<sakai::Time>,
  /// CPU usage in user mode, in nanoseconds.
  user: Quantity<sakai::Time>,
  /// CPU usage in kernel mode, in nanoseconds.
  system: Quantity<sakai::Time>,
  /// User time of positively niced tasks, included in user; since Linux 6.13.
  /// None means the kernel omitted the counter.
  nice: Option<Quantity<sakai::Time>>,
  /// Sibling time forced idle by core scheduling, separate from usage.
  /// Since Linux 6.0 with CONFIG_SCHED_CORE; None means omitted.
  force_idle: Option<Quantity<sakai::Time>>,
}

impl From<CoreCpuTimeStat> for CpuTimeStat {
  fn from(value: CoreCpuTimeStat) -> Self {
    Self {
      usage: value.usage().into(),
      user: value.user().into(),
      system: value.system().into(),
      nice: value.nice().map(|time| time.into()),
      force_idle: value.force_idle().map(|time| time.into()),
    }
  }
}

/// CPU bandwidth periods, throttling, and optional burst counters.
///
/// The kernel's signed 32-bit event counters are exposed as unsigned counts;
/// they wrap after 2^32 events, so snapshots need not stay monotonic forever.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CpuBandwidthStat {
  /// Number of elapsed bandwidth periods.
  nr_periods: Quantity<sakai::Count>,
  /// Number of bandwidth periods in which the cgroup was throttled.
  nr_throttled: Quantity<sakai::Count>,
  /// Cumulative throttled time in nanoseconds.
  throttled: Quantity<sakai::Time>,
  /// Burst counters, if the kernel reports them.
  burst: Option<CpuBurstStat>,
}

impl From<CoreCpuBandwidthStat> for CpuBandwidthStat {
  fn from(value: CoreCpuBandwidthStat) -> Self {
    Self {
      nr_periods: value.nr_periods().into(),
      nr_throttled: value.nr_throttled().into(),
      throttled: value.throttled().into(),
      burst: value.burst().map(Into::into),
    }
  }
}

/// CPU bandwidth burst counters.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CpuBurstStat {
  /// Number of bursts.
  nr_bursts: Quantity<sakai::Count>,
  /// Cumulative burst time in nanoseconds.
  burst: Quantity<sakai::Time>,
}

impl From<CoreCpuBurstStat> for CpuBurstStat {
  fn from(value: CoreCpuBurstStat) -> Self { Self { nr_bursts: value.nr_bursts().into(), burst: value.burst().into() } }
}

/// This cgroup's CPU quota and period, independent of ancestor limits.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Clone)]
pub(super) struct CpuMax {
  /// Quota in nanoseconds or the kernel's `max`.
  quota: MaxOr,
  /// Bandwidth period in nanoseconds.
  period: Quantity<sakai::Time>,
  /// Quota divided by period, or `max` at this cgroup only.
  cpu_count: MaxOr,
}

impl From<CoreCpuMax> for CpuMax {
  fn from(value: CoreCpuMax) -> Self {
    Self {
      quota: MaxOr::from_time(value.quota()),
      period: value.period().as_ref().into(),
      cpu_count: MaxOr::from_ratio(value.cpu_count()),
    }
  }
}

/// CPU scheduling weight or the special idle state.
#[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, get_all, skip_from_py_object, module = "sakai._sakai")]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CpuWeight {
  /// Whether this cgroup uses the special idle scheduling weight.
  is_idle: bool,
  /// Cgroup v2 weight from 1 through 10,000; None in the idle state.
  /// This is cpu.weight, distinct from cgroup v1 CPU shares.
  weight: Option<u16>,
}

impl From<CoreCpuWeight> for CpuWeight {
  fn from(value: CoreCpuWeight) -> Self {
    match value {
      | CoreCpuWeight::Idle => Self { is_idle: true, weight: None },
      | CoreCpuWeight::Shares(weight) => Self { is_idle: false, weight: Some(*weight.as_ref()) },
    }
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use sakai::{Bytes, MaxOr as CoreMaxOr};
  use uom::si::information::byte;

  use super::{super::limit::PythonLimitValue, *};

  #[test]
  fn converts_cpu_stat_optional_fields_and_nanoseconds() {
    let time =
      CpuTimeStat { usage: 10_000.into(), user: 7_000.into(), system: 3_000.into(), nice: None, force_idle: None };
    for (input, bandwidth) in [
      ("usage_usec 10\nuser_usec 7\nsystem_usec 3\n", None),
      (
        "usage_usec 10\nuser_usec 7\nsystem_usec 3\nnr_periods 5\nnr_throttled 2\nthrottled_usec 4\nnr_bursts \
         1\nburst_usec 6\n",
        Some(CpuBandwidthStat {
          nr_periods: 5.into(),
          nr_throttled: 2.into(),
          throttled: 4_000.into(),
          burst: Some(CpuBurstStat { nr_bursts: 1.into(), burst: 6_000.into() }),
        }),
      ),
    ] {
      assert_eq!(CpuStat::from(assert_ok!(input.parse::<CoreCpuStat>())), CpuStat { time: time.clone(), bandwidth });
    }
    for (input, throttled) in [("", None), ("throttled_usec 0", Some(0))] {
      assert_eq!(CpuStatLocal::from(assert_ok!(input.parse::<CoreCpuStatLocal>())), CpuStatLocal {
        throttled: throttled.map(Into::into)
      });
    }
    let input = "usage_usec 10\nuser_usec 7\nsystem_usec 3\nnice_usec 2\ncore_sched.force_idle_usec 0";
    assert_eq!(CpuTimeStat::from(assert_ok!(input.parse::<CoreCpuTimeStat>())), CpuTimeStat {
      nice: Some(2_000.into()),
      force_idle: Some(0.into()),
      ..time
    });
  }

  #[test]
  fn converts_limits_and_idle_without_collapsing_zero() {
    let unlimited = CpuMax::from(assert_ok!("max 100000".parse::<CoreCpuMax>()));
    let limited = CpuMax::from(assert_ok!("25000 100000".parse::<CoreCpuMax>()));
    assert!(unlimited.quota.is_max());
    assert_err!(unlimited.quota.value());
    assert!(unlimited.cpu_count.is_max());
    assert_err!(unlimited.cpu_count.value());
    assert!(matches!(assert_ok!(limited.quota.value()), PythonLimitValue::Time(value) if value == 25_000_000.into()));
    assert_eq!(limited.period, 100_000_000.into());
    assert!(matches!(assert_ok!(limited.cpu_count.value()), PythonLimitValue::Ratio(value) if value == 0.25.into()));

    let idle = CpuWeight::from(assert_ok!("0".parse::<CoreCpuWeight>()));
    let weighted = CpuWeight::from(assert_ok!("100".parse::<CoreCpuWeight>()));
    assert!(idle.is_idle);
    assert_eq!(idle.weight, None);
    assert!(!weighted.is_idle);
    assert_eq!(weighted.weight, Some(100));

    let max = MaxOr::from_bytes(CoreMaxOr::Max);
    assert!(max.is_max());
    assert_err!(max.value());
    assert!(matches!(
      assert_ok!(MaxOr::from_bytes(CoreMaxOr::Value(Bytes::new::<byte>(0))).value()),
      PythonLimitValue::Bytes(value) if value == 0.into()
    ));
  }
}
