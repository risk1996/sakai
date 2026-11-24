#![cfg(target_os = "linux")]

use std::{fmt::Debug, fs, io, path::PathBuf, process::Command, str::FromStr, time::Duration};

use assertables::{assert_ok, assert_some};
use sakai::{
  Bytes, Cgroup, Error, IrqPressure, MaxOr, NonZeroTime, Pressure, Time,
  error::{ParseError, ParseValueError},
  v2::{
    CgroupPath,
    core::{
      CgroupController, CgroupControllers, CgroupEvents, CgroupFreeze, CgroupMaxDepth, CgroupMaxDescendants,
      CgroupPressure, CgroupProcs, CgroupStat, CgroupStatLocal, CgroupSubtreeControl, CgroupThreads, CgroupType,
    },
    cpu::{
      CpuIdle, CpuMax, CpuMaxBurst, CpuPressure, CpuQuota, CpuStat, CpuStatLocal, CpuUclampMax, CpuUclampMin,
      CpuWeight, Nice,
    },
    cpuset::{
      CpuList, CpusEffective, CpusExclusive, CpusExclusiveEffective, CpusIsolated, CpusetPartition, MemoryNodeList,
      MemsEffective,
    },
    dmem::{DmemCapacity, DmemCurrent, DmemLow, DmemMax, DmemMin, DmemPeak},
    hugetlb::{
      HugetlbCurrent, HugetlbEvents, HugetlbEventsLocal, HugetlbMax, HugetlbNumaStat, HugetlbReservedCurrent,
      HugetlbReservedMax, PageSize,
    },
    io::{IoCostModel, IoCostQos, IoLatency, IoMax, IoPressure, IoPriorityClass, IoStat, IoWeight},
    memory::{
      MemoryCurrent, MemoryEvents, MemoryEventsLocal, MemoryHigh, MemoryLow, MemoryMax, MemoryMin, MemoryNumaStat,
      MemoryOomGroup, MemoryPeak, MemoryPressure, MemoryStat, SwapCurrent, SwapEvents, SwapHigh, SwapMax, SwapPeak,
      ZswapCurrent, ZswapMax, ZswapWriteback,
    },
    misc::{MiscCapacity, MiscCurrent, MiscEvents, MiscEventsLocal, MiscMax, MiscPeak},
    notification::{ConfigurationInterface, EventInterface},
    pids::{PidsCurrent, PidsEvents, PidsEventsLocal, PidsMax, PidsPeak},
    rdma::{RdmaCurrent, RdmaEvents, RdmaEventsLocal, RdmaMax, RdmaPeak},
  },
};
use strum::IntoEnumIterator;
use uom::si::{
  information::byte,
  time::{microsecond, nanosecond},
};

#[test]
fn reads_current_cgroup_without_privileges() {
  let cgroup = match Cgroup::from_current_process() {
    | Ok(cgroup) => cgroup,
    | Err(Error::NotCgroupV2) => return,
    | Err(error) => panic!("cgroup discovery failed: {error}"),
  };
  assert_ok!(cgroup.read_value::<CpuStat>());
  assert_ok!(cgroup.read_value::<CgroupControllers>());
  assert_ok!(cgroup.read_value::<CgroupSubtreeControl>());
  assert_ok!(cgroup.read_value::<CgroupStat>());
  for result in [
    cgroup.read_value::<CpuList>().map(|_| ()),
    cgroup.read_value::<CpusEffective>().map(|_| ()),
    cgroup.read_value::<MemoryNodeList>().map(|_| ()),
    cgroup.read_value::<MemsEffective>().map(|_| ()),
    cgroup.read_value::<CpusetPartition>().map(|_| ()),
    cgroup.read_value::<CpusExclusive>().map(|_| ()),
    cgroup.read_value::<CpusExclusiveEffective>().map(|_| ()),
    cgroup.read_value::<CpusIsolated>().map(|_| ()),
    cgroup.read_value::<IoStat>().map(|_| ()),
    cgroup.read_value::<IoPressure>().map(|_| ()),
    cgroup.read_value::<IoWeight>().map(|_| ()),
    cgroup.read_value::<IoMax>().map(|_| ()),
    cgroup.read_value::<IoLatency>().map(|_| ()),
    cgroup.read_value::<IoCostQos>().map(|_| ()),
    cgroup.read_value::<IoCostModel>().map(|_| ()),
    cgroup.read_value::<IoPriorityClass>().map(|_| ()),
    cgroup.read_value::<CpuStatLocal>().map(|_| ()),
    cgroup.read_value::<CpuWeight>().map(|_| ()),
    cgroup.read_value::<Nice>().map(|_| ()),
    cgroup.read_value::<CpuMax>().map(|_| ()),
    cgroup.read_value::<CpuMaxBurst>().map(|_| ()),
    cgroup.read_value::<CpuPressure>().map(|_| ()),
    cgroup.read_value::<CpuUclampMin>().map(|_| ()),
    cgroup.read_value::<CpuUclampMax>().map(|_| ()),
    cgroup.read_value::<CpuIdle>().map(|_| ()),
    cgroup.read_value::<CgroupType>().map(|_| ()),
    cgroup.read_value::<CgroupEvents>().map(|_| ()),
    cgroup.read_value::<CgroupProcs>().map(|_| ()),
    cgroup.read_value::<CgroupThreads>().map(|_| ()),
    cgroup.read_value::<CgroupStatLocal>().map(|_| ()),
    cgroup.read_value::<CgroupFreeze>().map(|_| ()),
    cgroup.read_value::<CgroupMaxDepth>().map(|_| ()),
    cgroup.read_value::<CgroupMaxDescendants>().map(|_| ()),
    cgroup.read_value::<CgroupPressure>().map(|_| ()),
    cgroup.read_value::<IrqPressure>().map(|_| ()),
    cgroup.read_value::<PidsCurrent>().map(|_| ()),
    cgroup.read_value::<PidsMax>().map(|_| ()),
    cgroup.read_value::<PidsEvents>().map(|_| ()),
    cgroup.read_value::<PidsPeak>().map(|_| ()),
    cgroup.read_value::<PidsEventsLocal>().map(|_| ()),
    cgroup.read_value::<MemoryCurrent>().map(|_| ()),
    cgroup.read_value::<MemoryMax>().map(|_| ()),
    cgroup.read_value::<MemoryHigh>().map(|_| ()),
    cgroup.read_value::<MemoryLow>().map(|_| ()),
    cgroup.read_value::<MemoryMin>().map(|_| ()),
    cgroup.read_value::<MemoryPeak>().map(|_| ()),
    cgroup.read_value::<MemoryStat>().map(|_| ()),
    cgroup.read_value::<MemoryEvents>().map(|_| ()),
    cgroup.read_value::<MemoryEventsLocal>().map(|_| ()),
    cgroup.read_value::<MemoryOomGroup>().map(|_| ()),
    cgroup.read_value::<MemoryNumaStat>().map(|_| ()),
    cgroup.read_value::<MemoryPressure>().map(|_| ()),
    cgroup.read_value::<SwapCurrent>().map(|_| ()),
    cgroup.read_value::<SwapPeak>().map(|_| ()),
    cgroup.read_value::<SwapMax>().map(|_| ()),
    cgroup.read_value::<SwapHigh>().map(|_| ()),
    cgroup.read_value::<SwapEvents>().map(|_| ()),
    cgroup.read_value::<ZswapCurrent>().map(|_| ()),
    cgroup.read_value::<ZswapMax>().map(|_| ()),
    cgroup.read_value::<ZswapWriteback>().map(|_| ()),
  ] {
    match result {
      | Ok(()) | Err(Error::FileMissing { .. } | Error::NotSupported { .. }) => {},
      | Err(error) => panic!("cgroup read failed: {error}"),
    }
  }
  assert_ok!(Cgroup::from_pid(std::process::id()));
  assert_ok!(cgroup.children());
  for interface in EventInterface::iter() {
    match cgroup.poll_events(interface) {
      | Ok(mut events) => {
        assert_ok!(events.wait(Some(Duration::ZERO)));
      },
      | Err(Error::FileMissing { path }) => assert_eq!(path, cgroup.path().join(interface.filename())),
      | Err(error) => panic!("event subscription failed: {error}"),
    }
  }
  for interface in ConfigurationInterface::iter() {
    match cgroup.watch_configuration(interface) {
      | Ok(mut configuration) => {
        assert_ok!(configuration.wait(Some(Duration::ZERO)));
      },
      | Err(Error::FileMissing { path }) => assert_eq!(path, cgroup.path().join(interface.filename())),
      | Err(error) => panic!("configuration subscription failed: {error}"),
    }
  }
}

#[test]
fn rejects_non_cgroup_filesystems() {
  assert!(matches!(Cgroup::from_path(std::path::Path::new("/")), Err(Error::NotCgroupV2)));
}

#[test]
fn parses_available_root_cpu_interfaces() {
  type Parse = fn(&str) -> Result<(), ParseError<ParseValueError>>;

  let parsers: [(&str, Parse); 10] = [
    (CpuIdle::FILE_NAME, |contents| contents.parse::<CpuIdle>().map(|_| ())),
    (CpuMax::FILE_NAME, |contents| contents.parse::<CpuMax>().map(|_| ())),
    (CpuMaxBurst::FILE_NAME, |contents| contents.parse::<CpuMaxBurst>().map(|_| ())),
    (Pressure::CPU_FILE_NAME, |contents| contents.parse::<Pressure>().map(|_| ())),
    (CpuStat::FILE_NAME, |contents| contents.parse::<CpuStat>().map(|_| ())),
    (CpuStatLocal::FILE_NAME, |contents| contents.parse::<CpuStatLocal>().map(|_| ())),
    (CpuUclampMax::FILE_NAME, |contents| contents.parse::<CpuUclampMax>().map(|_| ())),
    (CpuUclampMin::FILE_NAME, |contents| contents.parse::<CpuUclampMin>().map(|_| ())),
    (CpuWeight::FILE_NAME, |contents| contents.parse::<CpuWeight>().map(|_| ())),
    (Nice::FILE_NAME, |contents| contents.parse::<Nice>().map(|_| ())),
  ];

  for (file, parse) in parsers {
    let path = PathBuf::from("/sys/fs/cgroup").join(file);
    let contents = match fs::read_to_string(&path) {
      | Ok(contents) => contents,
      | Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
      | Err(error) => panic!("failed to read {path:?}: {error}"),
    };
    assert_ok!(parse(&contents), "path: {path:?}");
  }
}

#[derive(Debug)]
struct CgroupFixture {
  path: PathBuf,
}

impl CgroupFixture {
  /// Configures a disposable interface, then verifies fresh reads do not change it.
  fn configure<T>(&self, file: &str, input: &str, expected: T, read: impl FnOnce() -> Result<T, Error>)
  where
    T: Debug + PartialEq, {
    let path = self.path.join(file);
    assert_ok!(fs::write(&path, input), "path: {path:?}");
    assert_eq!(assert_ok!(read(), "path: {path:?}"), expected, "path: {path:?}");
    assert_eq!(assert_ok!(fs::read_to_string(&path)).trim(), input, "path: {path:?}");
  }

  /// Creates a child in vmtest's fresh, disposable cgroup2 hierarchy.
  fn create() -> io::Result<Self> {
    let cgroup = CgroupPath::current().map_err(io::Error::other)?;
    match cgroup.read_only {
      | true => {
        let status = Command::new("mount").args(["-o", "remount,rw"]).arg(&cgroup.mount_point).status()?;
        match status.success() {
          | true => {},
          | false => {
            return Err(io::Error::other(format!("cgroup remount failed: {status}")));
          },
        }
      },
      | false => {},
    }
    let parent = cgroup.as_ref();
    let path = parent.join(format!("sakai-vmtest-{}", std::process::id()));

    let controllers = fs::read_to_string(parent.join(CgroupController::CONTROLLERS_FILE_NAME))?;
    fs::write(parent.join(CgroupController::SUBTREE_CONTROL_FILE_NAME), "+cpu")?;
    fs::write(parent.join(CgroupController::SUBTREE_CONTROL_FILE_NAME), "+pids")?;
    for controller in ["memory", "io", "cpuset", "hugetlb", "rdma", "dmem", "misc"] {
      if controllers.split_ascii_whitespace().any(|available| available == controller) {
        fs::write(parent.join(CgroupController::SUBTREE_CONTROL_FILE_NAME), format!("+{controller}"))?;
      }
    }
    fs::create_dir(&path)?;
    let fixture = Self { path };
    fs::write(fixture.path.join(CpuMax::FILE_NAME), "25000 100000")?;

    Ok(fixture)
  }

  /// Checks that a raw interface file and its typed reader agree on presence.
  fn check<T: FromStr + Debug>(&self, file: &str, optional: bool, read: Result<T, Error>)
  where
    T::Err: Debug, {
    let path = self.path.join(file);
    let contents = match fs::read_to_string(&path) {
      | Err(error) if optional && error.kind() == io::ErrorKind::NotFound => {
        assert!(matches!(read, Err(Error::FileMissing { path: missing }) if missing == path));
        return;
      },
      | result => assert_ok!(result, "path: {path:?}"),
    };
    assert_ok!(contents.parse::<T>(), "path: {path:?}");
    assert_ok!(read, "path: {path:?}");
  }
}

impl Drop for CgroupFixture {
  fn drop(&mut self) { drop(fs::remove_dir(&self.path)); }
}

/// Reads real delegated files, including the memory values configured below.
#[test]
fn parses_live_delegated_controller_interfaces() {
  match std::env::var_os("SAKAI_VMTEST") {
    | None => return,
    | Some(_) => {},
  }

  let fixture = assert_ok!(CgroupFixture::create());
  let reader = assert_ok!(Cgroup::from_path(&fixture.path));
  assert_ok!(check_remaining_controllers(&reader));
  let controllers = assert_ok!(reader.read_value::<CgroupControllers>());
  if controllers.contains(&CgroupController::Rdma) {
    assert_ok!(reader.read_value::<RdmaCurrent>());
    assert_ok!(reader.read_value::<RdmaMax>());
  }
  if controllers.contains(&CgroupController::Dmem) {
    assert_ok!(reader.read_value::<DmemCurrent>());
    assert_ok!(reader.read_value::<DmemMin>());
    assert_ok!(reader.read_value::<DmemLow>());
    assert_ok!(reader.read_value::<DmemMax>());
  }
  if controllers.contains(&CgroupController::Misc) {
    assert_ok!(reader.read_value::<MiscCurrent>());
    assert_ok!(reader.read_value::<MiscMax>());
  }
  if controllers.contains(&CgroupController::Hugetlb) {
    let size = assert_ok!("2MB".parse::<PageSize>());
    assert_ok!(reader.read_file::<HugetlbCurrent>(&size));
    assert_ok!(reader.read_file::<HugetlbMax>(&size));
    assert_ok!(reader.read_file::<HugetlbReservedCurrent>(&size));
    assert_ok!(reader.read_file::<HugetlbReservedMax>(&size));
    assert_ok!(reader.read_file::<HugetlbEvents>(&size));
    assert_ok!(reader.read_file::<HugetlbEventsLocal>(&size));
  }

  let child_path = fixture.path.join("cpu.future");
  assert_ok!(fs::create_dir(&child_path));
  let child = assert_ok!(reader.child(std::ffi::OsStr::new("cpu.future")));
  assert!(assert_ok!(reader.children()).iter().any(|entry| entry.path() == child_path));
  assert_ok!(child.read_value::<CpuStat>());
  assert_ok!(fs::remove_dir(&child_path));
  let mut limit = assert_ok!(reader.read_value::<CpuMax>());
  assert_eq!(limit, assert_ok!("25000 100000".parse::<CpuMax>()));
  limit.quota = MaxOr::Value(assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(30_000))));
  limit.period = assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(50_000)));
  assert_ok!(reader.write_value::<CpuMax>(&limit));
  assert_eq!(assert_ok!(reader.read_value::<CpuMax>()), limit);
  assert_eq!(assert_ok!(fs::read_to_string(fixture.path.join(CpuMax::FILE_NAME))).trim(), "30000 50000");
  let mut unlimited = limit;
  unlimited.quota = MaxOr::Max;
  assert_ok!(reader.write_value::<CpuQuota>(unlimited.quota));
  assert_eq!(assert_ok!(reader.read_value::<CpuMax>()), unlimited);

  let path = fixture.path.join(CpuMax::FILE_NAME);
  let mut invalid = limit;
  invalid.period = assert_ok!(NonZeroTime::try_new(Time::new::<nanosecond>(50_000_001)));
  assert!(matches!(
    reader.write_value::<CpuMax>(&invalid),
    Err(Error::Encode { path: actual, source }) if actual == path && source.field() == "period"
  ));
  invalid.period = assert_ok!(NonZeroTime::try_new(Time::new::<microsecond>(1)));
  assert!(matches!(
    reader.write_value::<CpuMax>(&invalid),
    Err(Error::Operation { operation: "write interface", path: actual, source })
      if actual == path && source.kind() == io::ErrorKind::InvalidInput && source.raw_os_error().is_some()
  ));
  assert_eq!(assert_ok!(reader.read_value::<CpuMax>()), unlimited);
  assert_ok!(reader.read_value::<CpuStat>());
  match reader.read_value::<CpuStatLocal>() {
    | Ok(_) | Err(Error::FileMissing { .. }) => {},
    | Err(error) => panic!("local CPU read failed: {error}"),
  }

  match assert_ok!(reader.read_value::<CgroupControllers>()).contains(&CgroupController::Cpuset) {
    | true => {
      fixture.check(CpuList::CPUS_FILE_NAME, false, reader.read_value::<CpuList>());
      fixture.check(CpuList::EFFECTIVE_FILE_NAME, false, reader.read_value::<CpusEffective>());
      fixture.check(MemoryNodeList::MEMS_FILE_NAME, false, reader.read_value::<MemoryNodeList>());
      fixture.check(MemoryNodeList::EFFECTIVE_FILE_NAME, false, reader.read_value::<MemsEffective>());
      fixture.check(CpusetPartition::FILE_NAME, true, reader.read_value::<CpusetPartition>());
      assert_eq!(assert_ok!(reader.read_value::<CpuList>()), CpuList::default());
      assert_eq!(assert_ok!(reader.read_value::<MemoryNodeList>()), MemoryNodeList::default());
      // Configure a granted resource, then check a fresh read and unchanged file.
      // No process enters this fixture; these writes only exercise the test hierarchy.
      let cpu = assert_ok!(reader.read_value::<CpusEffective>()).ids().next().expect("online CPU").to_string();
      let node = assert_ok!(reader.read_value::<MemsEffective>()).ids().next().expect("online memory node").to_string();
      let cpu = assert_ok!(cpu.parse::<CpuList>());
      let node = assert_ok!(node.parse::<MemoryNodeList>());
      assert_ok!(reader.write_value::<CpuList>(&cpu));
      assert_ok!(reader.write_value::<MemoryNodeList>(&node));
      assert_eq!(assert_ok!(reader.read_value::<CpuList>()), cpu);
      assert_eq!(assert_ok!(reader.read_value::<MemoryNodeList>()), node);
      assert_ok!(reader.write_value::<CpuList>(&CpuList::default()));
      assert_ok!(reader.write_value::<MemoryNodeList>(&MemoryNodeList::default()));
      assert_eq!(assert_ok!(reader.read_value::<CpuList>()), CpuList::default());
      assert_eq!(assert_ok!(reader.read_value::<MemoryNodeList>()), MemoryNodeList::default());
    },
    | false => {},
  }

  fixture.check(CpuIdle::FILE_NAME, true, reader.read_value::<CpuIdle>());
  fixture.check(CpuMax::FILE_NAME, false, reader.read_value::<CpuMax>());
  fixture.check(CpuMaxBurst::FILE_NAME, true, reader.read_value::<CpuMaxBurst>());
  fixture.check(Pressure::CPU_FILE_NAME, false, reader.read_value::<CpuPressure>());
  fixture.check(CpuStat::FILE_NAME, false, reader.read_value::<CpuStat>());
  fixture.check(CpuStatLocal::FILE_NAME, true, reader.read_value::<CpuStatLocal>());
  fixture.check(CpuUclampMax::FILE_NAME, true, reader.read_value::<CpuUclampMax>());
  fixture.check(CpuUclampMin::FILE_NAME, true, reader.read_value::<CpuUclampMin>());
  fixture.check(CpuWeight::FILE_NAME, false, reader.read_value::<CpuWeight>());
  fixture.check(Nice::FILE_NAME, false, reader.read_value::<Nice>());
  fixture.check(PidsCurrent::FILE_NAME, false, reader.read_value::<PidsCurrent>());
  fixture.check(PidsMax::FILE_NAME, false, reader.read_value::<PidsMax>());
  fixture.check(PidsEvents::FILE_NAME, false, reader.read_value::<PidsEvents>());
  fixture.check(CgroupEvents::FILE_NAME, false, reader.read_value::<CgroupEvents>());
  fixture.check(CgroupStat::FILE_NAME, false, reader.read_value::<CgroupStat>());
  assert!(
    assert_ok!(reader.read_value::<CgroupControllers>()).contains(&CgroupController::Io),
    "vmtest kernel must provide the I/O controller"
  );
  fixture.check(IoStat::FILE_NAME, false, reader.read_value::<IoStat>());
  fixture.check(Pressure::IO_FILE_NAME, true, reader.read_value::<IoPressure>());
  fixture.check(IoWeight::FILE_NAME, true, reader.read_value::<IoWeight>());
  fixture.check(IoMax::FILE_NAME, false, reader.read_value::<IoMax>());
  fixture.check(IoLatency::FILE_NAME, true, reader.read_value::<IoLatency>());
  assert_eq!(assert_ok!(reader.read_value::<IoStat>()), IoStat::default());
  assert_eq!(assert_ok!(reader.read_value::<IoMax>()), IoMax::default());
  let weight_path = fixture.path.join(IoWeight::FILE_NAME);
  match fs::read_to_string(&weight_path) {
    | Ok(_) => {
      for weight in [1, 100, 10_000] {
        let configuration = format!("default {weight}");
        fixture.configure(IoWeight::FILE_NAME, &configuration, assert_ok!(configuration.parse::<IoWeight>()), || {
          reader.read_value::<IoWeight>()
        });
      }
    },
    | Err(error) if error.kind() == io::ErrorKind::NotFound => {},
    | Err(error) => panic!("failed to read {weight_path:?}: {error}"),
  }
  assert_eq!(assert_ok!(reader.read_value::<PidsCurrent>()).value.value, 0);
  assert_eq!(assert_ok!(reader.read_value::<PidsMax>()).value, MaxOr::Max);
  for limit in ["0", "10", "max"] {
    fixture
      .configure(PidsMax::FILE_NAME, limit, assert_ok!(limit.parse::<PidsMax>()), || reader.read_value::<PidsMax>());
  }
  assert_eq!(assert_ok!(reader.read_value::<PidsEvents>()).max.value, 0);
  let events = assert_ok!(reader.read_value::<CgroupEvents>());
  assert!(!events.populated);
  assert_eq!(events.frozen, Some(false));
  assert_eq!(assert_ok!(reader.read_value::<CgroupStat>()).descendants.value, 0);
  assert!(
    assert_ok!(reader.read_value::<CgroupControllers>()).contains(&CgroupController::Memory),
    "vmtest kernel must provide the memory controller"
  );
  fixture.check(MemoryCurrent::FILE_NAME, false, reader.read_value::<MemoryCurrent>());
  fixture.check(MemoryMax::FILE_NAME, false, reader.read_value::<MemoryMax>());
  fixture.check(MemoryHigh::FILE_NAME, false, reader.read_value::<MemoryHigh>());
  fixture.check(MemoryLow::FILE_NAME, false, reader.read_value::<MemoryLow>());
  fixture.check(MemoryMin::FILE_NAME, false, reader.read_value::<MemoryMin>());
  fixture.check(MemoryPeak::FILE_NAME, true, reader.read_value::<MemoryPeak>());
  fixture.check(MemoryStat::FILE_NAME, false, reader.read_value::<MemoryStat>());
  fixture.check(MemoryEvents::FILE_NAME, false, reader.read_value::<MemoryEvents>());
  fixture.check(MemoryEventsLocal::FILE_NAME, true, reader.read_value::<MemoryEventsLocal>());
  fixture.check(MemoryOomGroup::FILE_NAME, false, reader.read_value::<MemoryOomGroup>());
  fixture.check(MemoryNumaStat::FILE_NAME, true, reader.read_value::<MemoryNumaStat>());
  fixture.check(Pressure::MEMORY_FILE_NAME, true, reader.read_value::<MemoryPressure>());
  fixture.check(SwapCurrent::FILE_NAME, true, reader.read_value::<SwapCurrent>());
  fixture.check(SwapPeak::FILE_NAME, true, reader.read_value::<SwapPeak>());
  fixture.check(SwapMax::FILE_NAME, true, reader.read_value::<SwapMax>());
  fixture.check(SwapHigh::FILE_NAME, true, reader.read_value::<SwapHigh>());
  fixture.check(SwapEvents::FILE_NAME, true, reader.read_value::<SwapEvents>());
  fixture.check(ZswapCurrent::FILE_NAME, true, reader.read_value::<ZswapCurrent>());
  fixture.check(ZswapMax::FILE_NAME, true, reader.read_value::<ZswapMax>());
  fixture.check(ZswapWriteback::FILE_NAME, true, reader.read_value::<ZswapWriteback>());

  // No process joins the fixture, so its usage is stable across these reads.
  let current_contents = assert_ok!(fs::read_to_string(fixture.path.join(MemoryCurrent::FILE_NAME)));
  let current_bytes = assert_ok!(current_contents.trim().parse::<u64>());
  assert_eq!(assert_ok!(reader.read_value::<MemoryCurrent>()).value.get::<byte>(), current_bytes);

  assert_eq!(assert_ok!(reader.read_value::<MemoryMax>()).value, MaxOr::Max);
  assert_eq!(assert_ok!(reader.read_value::<MemoryHigh>()).value, MaxOr::Max);
  assert_eq!(assert_ok!(reader.read_value::<MemoryLow>()).value, MaxOr::Value(Bytes::new::<byte>(0)));
  assert_eq!(assert_ok!(reader.read_value::<MemoryMin>()).value, MaxOr::Value(Bytes::new::<byte>(0)));
  assert!(!assert_ok!(reader.read_value::<MemoryOomGroup>()).value);
  for (input, expected) in [("1", true), ("0", false)] {
    fixture.configure(MemoryOomGroup::FILE_NAME, input, expected, || {
      reader.read_value::<MemoryOomGroup>().map(|value| value.value)
    });
  }
  for limit in [MaxOr::Value(Bytes::new::<byte>(67_108_864)), MaxOr::Max] {
    assert_ok!(reader.write_value::<MemoryMax>(limit));
    assert_eq!(assert_ok!(reader.read_value::<MemoryMax>()).value, limit);
  }
  fixture.configure(MemoryHigh::FILE_NAME, "33554432", MaxOr::Value(Bytes::new::<byte>(33_554_432)), || {
    reader.read_value::<MemoryHigh>().map(|value| value.value)
  });
  fixture.configure(MemoryLow::FILE_NAME, "16777216", MaxOr::Value(Bytes::new::<byte>(16_777_216)), || {
    reader.read_value::<MemoryLow>().map(|value| value.value)
  });
  fixture.configure(MemoryMin::FILE_NAME, "8388608", MaxOr::Value(Bytes::new::<byte>(8_388_608)), || {
    reader.read_value::<MemoryMin>().map(|value| value.value)
  });
  for file in [MemoryLow::FILE_NAME, MemoryMin::FILE_NAME] {
    assert_ok!(fs::write(fixture.path.join(file), "max"));
  }
  assert_eq!(assert_ok!(reader.read_value::<MemoryLow>()).value, MaxOr::Max);
  assert_eq!(assert_ok!(reader.read_value::<MemoryMin>()).value, MaxOr::Max);
  assert_eq!(assert_ok!(reader.read_value::<CgroupType>()), CgroupType::Domain);
  assert_ok!(reader.read_value::<CgroupControllers>());
  assert_ok!(reader.read_value::<CgroupSubtreeControl>());

  // Notifications only observe this disposable delegated test hierarchy.
  assert_ok!(fs::write(fixture.path.join(CgroupController::SUBTREE_CONTROL_FILE_NAME), "+cpu"));
  let notification_path = fixture.path.join("notifications");
  assert_ok!(fs::create_dir(&notification_path));
  let notification_group = assert_ok!(reader.child(std::ffi::OsStr::new("notifications")));
  let mut events = assert_ok!(notification_group.poll_events(EventInterface::CgroupEvents));
  let mut configuration = assert_ok!(notification_group.watch_configuration(ConfigurationInterface::CpuMax));
  let notification_timeout = Some(Duration::from_secs(2));
  assert!(!assert_ok!(events.wait(Some(Duration::ZERO))));
  assert_eq!(assert_ok!(configuration.wait(Some(Duration::ZERO))), None);

  for frozen in [true, false] {
    assert_ok!(fs::write(notification_path.join("cgroup.freeze"), match frozen {
      | true => "1",
      | false => "0",
    }));
    assert!(assert_ok!(events.wait(notification_timeout)));
    assert_eq!(assert_ok!(notification_group.read_value::<CgroupEvents>()).frozen, Some(frozen));
    assert!(!assert_ok!(events.wait(Some(Duration::ZERO))));
  }
  for contents in ["25000 100000", "max 100000"] {
    assert_ok!(fs::write(notification_path.join(CpuMax::FILE_NAME), contents));
    let change = assert_ok!(configuration.wait(notification_timeout)).expect("configuration notification");
    assert!(change.modified);
    assert!(!change.removed);
    assert!(!change.overflowed);
    assert_eq!(assert_ok!(notification_group.read_value::<CpuMax>()), assert_ok!(contents.parse::<CpuMax>()));
    assert_eq!(assert_ok!(fs::read_to_string(notification_path.join(CpuMax::FILE_NAME))).trim(), contents);
    assert_eq!(assert_ok!(configuration.wait(Some(Duration::ZERO))), None);
  }
  drop(events);
  drop(notification_group);
  assert_ok!(fs::remove_dir(&notification_path));
  // Kernfs internal removal need not emit an inotify deletion event. A
  // notification, when supplied by the kernel, must describe watch removal.
  match assert_ok!(configuration.wait(Some(Duration::ZERO))) {
    | Some(change) => assert!(change.removed),
    | None => {},
  }
}

#[test]
fn reads_root_io_interfaces_and_reports_root_exemptions() {
  match std::env::var_os("SAKAI_VMTEST") {
    | None => return,
    | Some(_) => {},
  }
  let path = PathBuf::from("/sys/fs/cgroup");
  let root = assert_ok!(Cgroup::from_path(&path));
  for (name, result) in [
    (IoWeight::FILE_NAME, root.read_value::<IoWeight>().map(|_| ())),
    (IoMax::FILE_NAME, root.read_value::<IoMax>().map(|_| ())),
    (IoLatency::FILE_NAME, root.read_value::<IoLatency>().map(|_| ())),
  ] {
    assert!(matches!(result, Err(Error::FileMissing { path: missing }) if missing == path.join(name)));
  }
  // Older kernels omit root io.stat; PSI can be disabled at build/boot time.
  for (name, result) in [
    (IoStat::FILE_NAME, root.read_value::<IoStat>().map(|_| ())),
    (Pressure::IO_FILE_NAME, root.read_value::<IoPressure>().map(|_| ())),
  ] {
    match fs::read_to_string(path.join(name)) {
      | Ok(_) => assert_ok!(result),
      | Err(error) if error.kind() == io::ErrorKind::NotFound => {
        assert!(matches!(result, Err(Error::FileMissing { path: missing }) if missing == path.join(name)));
      },
      | Err(error) => panic!("failed to read {name}: {error}"),
    }
  }
}

#[test]
fn root_memory_stat_is_readable_and_settings_are_missing() {
  match std::env::var_os("SAKAI_VMTEST") {
    | None => return,
    | Some(_) => {},
  }

  let root_path = PathBuf::from("/sys/fs/cgroup");
  let root = assert_ok!(Cgroup::from_path(&root_path));
  for (result, name) in [
    (root.read_value::<MemoryCurrent>().map(|_| ()), MemoryCurrent::FILE_NAME),
    (root.read_value::<MemoryMax>().map(|_| ()), MemoryMax::FILE_NAME),
    (root.read_value::<MemoryHigh>().map(|_| ()), MemoryHigh::FILE_NAME),
    (root.read_value::<MemoryLow>().map(|_| ()), MemoryLow::FILE_NAME),
    (root.read_value::<MemoryMin>().map(|_| ()), MemoryMin::FILE_NAME),
    (root.read_value::<MemoryPeak>().map(|_| ()), MemoryPeak::FILE_NAME),
    (root.read_value::<MemoryEvents>().map(|_| ()), MemoryEvents::FILE_NAME),
    (root.read_value::<MemoryEventsLocal>().map(|_| ()), MemoryEventsLocal::FILE_NAME),
    (root.read_value::<MemoryOomGroup>().map(|_| ()), MemoryOomGroup::FILE_NAME),
    (root.read_value::<SwapCurrent>().map(|_| ()), SwapCurrent::FILE_NAME),
    (root.read_value::<SwapPeak>().map(|_| ()), SwapPeak::FILE_NAME),
    (root.read_value::<SwapMax>().map(|_| ()), SwapMax::FILE_NAME),
    (root.read_value::<SwapHigh>().map(|_| ()), SwapHigh::FILE_NAME),
    (root.read_value::<SwapEvents>().map(|_| ()), SwapEvents::FILE_NAME),
    (root.read_value::<ZswapCurrent>().map(|_| ()), ZswapCurrent::FILE_NAME),
    (root.read_value::<ZswapMax>().map(|_| ()), ZswapMax::FILE_NAME),
  ] {
    assert!(matches!(result, Err(Error::FileMissing { path }) if path == root_path.join(name)));
  }
  assert_ok!(root.read_value::<MemoryStat>());
  assert_ok!(root.read_value::<CgroupStat>());
  for (name, read) in [
    (CgroupEvents::FILE_NAME, root.read_value::<CgroupEvents>().map(|_| ())),
    (PidsMax::FILE_NAME, root.read_value::<PidsMax>().map(|_| ())),
    (PidsEvents::FILE_NAME, root.read_value::<PidsEvents>().map(|_| ())),
  ] {
    assert!(matches!(read, Err(Error::FileMissing { path }) if path == root_path.join(name)));
  }
  for (name, read) in [
    (MemoryNumaStat::FILE_NAME, root.read_value::<MemoryNumaStat>().map(|_| ())),
    (ZswapWriteback::FILE_NAME, root.read_value::<ZswapWriteback>().map(|_| ())),
  ] {
    match fs::read_to_string(root_path.join(name)) {
      | Ok(_) => assert_ok!(read),
      | Err(error) if error.kind() == io::ErrorKind::NotFound => {
        assert!(matches!(read, Err(Error::FileMissing { .. })));
      },
      | Err(error) => panic!("failed to read {name}: {error}"),
    }
  }
}

#[test]
fn reads_root_placement_and_reports_configuration_exemptions() {
  match std::env::var_os("SAKAI_VMTEST") {
    | None => return,
    | Some(_) => {},
  }
  let path = PathBuf::from("/sys/fs/cgroup");
  let root = assert_ok!(Cgroup::from_path(&path));
  match assert_ok!(root.read_value::<CgroupControllers>()).contains(&CgroupController::Cpuset) {
    | true => {
      assert_ok!(root.read_value::<CpusEffective>());
      assert_ok!(root.read_value::<MemsEffective>());
      for (name, result) in [
        (CpuList::CPUS_FILE_NAME, root.read_value::<CpuList>().map(|_| ())),
        (MemoryNodeList::MEMS_FILE_NAME, root.read_value::<MemoryNodeList>().map(|_| ())),
        (CpusetPartition::FILE_NAME, root.read_value::<CpusetPartition>().map(|_| ())),
      ] {
        assert!(matches!(result, Err(Error::FileMissing { path: missing }) if missing == path.join(name)));
      }
    },
    | false => {},
  }
}

#[test]
fn reads_remaining_controllers_and_reports_unavailable_interfaces() {
  let group = match Cgroup::from_current_process() {
    | Ok(group) => group,
    | Err(Error::NotCgroupV2) => return,
    | Err(error) => panic!("cgroup discovery failed: {error}"),
  };
  assert_ok!(check_remaining_controllers(&group));
  let membership = assert_ok!(CgroupPath::current());
  let root = assert_ok!(Cgroup::from_path(&membership.mount_point));
  assert_ok!(check_remaining_controllers(&root));
}

fn check_remaining_controllers(group: &Cgroup) -> io::Result<()> {
  for (name, result) in [
    ("misc.capacity", group.read_value::<MiscCapacity>().map(|_| ())),
    ("misc.current", group.read_value::<MiscCurrent>().map(|_| ())),
    ("misc.peak", group.read_value::<MiscPeak>().map(|_| ())),
    ("misc.max", group.read_value::<MiscMax>().map(|_| ())),
    ("misc.events", group.read_value::<MiscEvents>().map(|_| ())),
    ("misc.events.local", group.read_value::<MiscEventsLocal>().map(|_| ())),
    ("dmem.capacity", group.read_value::<DmemCapacity>().map(|_| ())),
    ("dmem.current", group.read_value::<DmemCurrent>().map(|_| ())),
    (DmemPeak::FILE_NAME, group.read_value::<DmemPeak>().map(|_| ())),
    ("dmem.min", group.read_value::<DmemMin>().map(|_| ())),
    ("dmem.low", group.read_value::<DmemLow>().map(|_| ())),
    ("dmem.max", group.read_value::<DmemMax>().map(|_| ())),
    ("rdma.current", group.read_value::<RdmaCurrent>().map(|_| ())),
    ("rdma.peak", group.read_value::<RdmaPeak>().map(|_| ())),
    ("rdma.max", group.read_value::<RdmaMax>().map(|_| ())),
    ("rdma.events", group.read_value::<RdmaEvents>().map(|_| ())),
    ("rdma.events.local", group.read_value::<RdmaEventsLocal>().map(|_| ())),
  ] {
    check_present_or_missing(group, name, result)?;
  }
  // Discover available size labels from sysfs, independent of controller files.
  // This checks missing controller/root files as well as supported interfaces.
  let directory = match fs::read_dir("/sys/kernel/mm/hugepages") {
    | Ok(directory) => directory,
    | Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
    | Err(error) => return Err(error),
  };
  for entry in directory {
    let name = assert_ok!(entry).file_name();
    let name = name.to_string_lossy();
    let Some(kilobytes) = name.strip_prefix("hugepages-").and_then(|name| name.strip_suffix("kB")) else {
      continue;
    };
    let bytes = assert_some!(assert_ok!(kilobytes.parse::<u64>()).checked_mul(1024));
    let size = assert_ok!(PageSize::try_new(Bytes::new::<byte>(bytes)));
    for (suffix, result) in [
      ("current", group.read_file::<HugetlbCurrent>(&size).map(|_| ())),
      ("max", group.read_file::<HugetlbMax>(&size).map(|_| ())),
      ("rsvd.current", group.read_file::<HugetlbReservedCurrent>(&size).map(|_| ())),
      ("rsvd.max", group.read_file::<HugetlbReservedMax>(&size).map(|_| ())),
      ("events", group.read_file::<HugetlbEvents>(&size).map(|_| ())),
      ("events.local", group.read_file::<HugetlbEventsLocal>(&size).map(|_| ())),
      ("numa_stat", group.read_file::<HugetlbNumaStat>(&size).map(|_| ())),
    ] {
      check_present_or_missing(group, &format!("hugetlb.{size}.{suffix}"), result)?;
    }
  }
  Ok(())
}

fn check_present_or_missing(group: &Cgroup, name: &str, result: Result<(), Error>) -> io::Result<()> {
  let path = group.path().join(name);
  match fs::read_to_string(&path) {
    | Ok(_) => result.map_err(io::Error::other)?,
    | Err(error) if error.kind() == io::ErrorKind::NotFound => match result {
      | Err(Error::FileMissing { path: missing }) if missing == path => {},
      | unexpected => return Err(io::Error::other(format!("expected FileMissing at {path:?}, got {unexpected:?}"))),
    },
    | Err(error) => return Err(error),
  }
  Ok(())
}
