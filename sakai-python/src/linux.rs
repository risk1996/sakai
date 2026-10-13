//! Linux cgroup v2 Python bindings.

use pyo3::{prelude::*, types::PyModule};

use self::{
  core::{CgroupEvents, CgroupStat, CgroupStatLocal, CoreReader},
  cpu::{CpuBandwidthStat, CpuBurstStat, CpuMax, CpuReader, CpuStat, CpuStatLocal, CpuTimeStat, CpuWeight},
  cpuset::{CpuList, CpusetPartition, CpusetReader, MemoryNodeList},
  dmem::{DmemCapacity, DmemCurrent, DmemLimits, DmemPeak, DmemReader},
  errors::{
    CgroupParseError, DeletedCgroupError, InterfaceMissingError, NotCgroupV2Error, NotSupportedError, PythonError,
    SakaiError,
  },
  handle::Cgroup,
  hugetlb::{HugetlbEvents, HugetlbNumaStat, HugetlbReader},
  io::{
    IoCostModel, IoCostQos, IoDeviceCostModel, IoDeviceCostQos, IoDeviceMax, IoDeviceStat, IoLatency, IoMax, IoReader,
    IoStat, IoWeight,
  },
  limit::MaxOr,
  memory::{MemoryEventCounts, MemoryNumaStat, MemoryReader, MemoryStat, SwapEvents, SwapReader, ZswapReader},
  misc::{MiscCapacity, MiscCurrent, MiscEvents, MiscMax, MiscPeak, MiscReader},
  notification::{ChangeNotification, ConfigurationWatcher, EventPoller},
  pids::{PidsEvents, PidsReader},
  pressure::{IrqPressure, Pressure, PressureLine},
  rdma::{
    RdmaCurrent, RdmaDeviceCurrent, RdmaDeviceEvents, RdmaDeviceMax, RdmaEvents, RdmaMax, RdmaReader,
    RdmaResourceEvents,
  },
};

mod core;
mod cpu;
mod cpuset;
mod dmem;
mod errors;
mod handle;
mod hugetlb;
mod io;
mod limit;
mod mapping;
mod memory;
mod misc;
mod notification;
mod pids;
mod pressure;
mod rdma;
#[cfg(test)]
mod tests;

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
  module.add_class::<EventPoller>()?;
  module.add_class::<ConfigurationWatcher>()?;
  module.add_class::<ChangeNotification>()?;
  module.add_class::<HugetlbReader>()?;
  module.add_class::<HugetlbEvents>()?;
  module.add_class::<HugetlbNumaStat>()?;
  module.add_class::<RdmaReader>()?;
  module.add_class::<RdmaCurrent>()?;
  module.add_class::<RdmaDeviceCurrent>()?;
  module.add_class::<RdmaMax>()?;
  module.add_class::<RdmaDeviceMax>()?;
  module.add_class::<RdmaEvents>()?;
  module.add_class::<RdmaDeviceEvents>()?;
  module.add_class::<RdmaResourceEvents>()?;
  module.add_class::<DmemReader>()?;
  module.add_class::<DmemCapacity>()?;
  module.add_class::<DmemCurrent>()?;
  module.add_class::<DmemPeak>()?;
  module.add_class::<DmemLimits>()?;
  module.add_class::<MiscReader>()?;
  module.add_class::<MiscCapacity>()?;
  module.add_class::<MiscCurrent>()?;
  module.add_class::<MiscPeak>()?;
  module.add_class::<MiscMax>()?;
  module.add_class::<MiscEvents>()?;
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
  module.add_class::<IoCostQos>()?;
  module.add_class::<IoDeviceCostQos>()?;
  module.add_class::<IoCostModel>()?;
  module.add_class::<IoDeviceCostModel>()?;
  module.add_class::<SwapReader>()?;
  module.add_class::<ZswapReader>()?;
  module.add_class::<CoreReader>()?;
  module.add_class::<PidsReader>()?;
  module.add_class::<PidsEvents>()?;
  module.add_class::<CgroupEvents>()?;
  module.add_class::<CgroupStat>()?;
  module.add_class::<CgroupStatLocal>()?;
  module.add_class::<CpuStat>()?;
  module.add_class::<CpuStatLocal>()?;
  module.add_class::<CpuTimeStat>()?;
  module.add_class::<CpuBandwidthStat>()?;
  module.add_class::<CpuBurstStat>()?;
  module.add_class::<CpuMax>()?;
  module.add_class::<CpuWeight>()?;
  module.add_class::<Pressure>()?;
  module.add_class::<IrqPressure>()?;
  module.add_class::<PressureLine>()?;
  module.add_class::<MemoryStat>()?;
  module.add_class::<MemoryEventCounts>()?;
  module.add_class::<SwapEvents>()?;
  module.add_class::<MemoryNumaStat>()?;
  Ok(())
}
