//! Linux cgroup v2 Python bindings.

use pyo3::{prelude::*, types::PyModule};

use self::{
  core::{CgroupEvents, CgroupStat, CoreReader},
  cpu::{CpuBandwidthStat, CpuBurstStat, CpuMax, CpuReader, CpuStat, CpuStatLocal, CpuTimeStat, CpuWeight},
  cpuset::{CpuList, CpusetPartition, CpusetReader, MemoryNodeList},
  errors::{
    CgroupParseError, DeletedCgroupError, InterfaceMissingError, NotCgroupV2Error, NotSupportedError, PythonError,
    SakaiError,
  },
  handle::Cgroup,
  io::{IoDeviceMax, IoDeviceStat, IoLatency, IoMax, IoReader, IoStat, IoWeight},
  limit::MaxOr,
  memory::{MemoryEventCounts, MemoryNumaStat, MemoryReader, MemoryStat, SwapEvents, SwapReader, ZswapReader},
  pids::{PidsEvents, PidsReader},
  pressure::{Pressure, PressureLine},
};

mod core;
mod cpu;
mod cpuset;
mod errors;
mod handle;
mod io;
mod limit;
mod memory;
mod pids;
mod pressure;

/// Read-only Linux cgroup v2 bindings backed by sakai.
#[pymodule(gil_used = true)]
fn _sakai(module: &Bound<'_, PyModule>) -> PyResult<()> {
  module.add("SakaiError", module.py().get_type::<SakaiError>())?;
  module.add("InterfaceMissingError", module.py().get_type::<InterfaceMissingError>())?;
  module.add("NotCgroupV2Error", module.py().get_type::<NotCgroupV2Error>())?;
  module.add("DeletedCgroupError", module.py().get_type::<DeletedCgroupError>())?;
  module.add("CgroupParseError", module.py().get_type::<CgroupParseError>())?;
  module.add("NotSupportedError", module.py().get_type::<NotSupportedError>())?;
  module.add_class::<Cgroup>()?;
  module.add_class::<MaxOr>()?;
  module.add_class::<CpuReader>()?;
  module.add_class::<MemoryReader>()?;
  module.add_class::<CpusetReader>()?;
  module.add_class::<CpuList>()?;
  module.add_class::<MemoryNodeList>()?;
  module.add_class::<CpusetPartition>()?;
  module.add_class::<IoReader>()?;
  module.add_class::<IoStat>()?;
  module.add_class::<IoDeviceStat>()?;
  module.add_class::<IoWeight>()?;
  module.add_class::<IoMax>()?;
  module.add_class::<IoDeviceMax>()?;
  module.add_class::<IoLatency>()?;
  module.add_class::<SwapReader>()?;
  module.add_class::<ZswapReader>()?;
  module.add_class::<CoreReader>()?;
  module.add_class::<PidsReader>()?;
  module.add_class::<PidsEvents>()?;
  module.add_class::<CgroupEvents>()?;
  module.add_class::<CgroupStat>()?;
  module.add_class::<CpuStat>()?;
  module.add_class::<CpuStatLocal>()?;
  module.add_class::<CpuTimeStat>()?;
  module.add_class::<CpuBandwidthStat>()?;
  module.add_class::<CpuBurstStat>()?;
  module.add_class::<CpuMax>()?;
  module.add_class::<CpuWeight>()?;
  module.add_class::<Pressure>()?;
  module.add_class::<PressureLine>()?;
  module.add_class::<MemoryStat>()?;
  module.add_class::<MemoryEventCounts>()?;
  module.add_class::<SwapEvents>()?;
  module.add_class::<MemoryNumaStat>()?;
  Ok(())
}
