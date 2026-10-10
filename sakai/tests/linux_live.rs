#![cfg(target_os = "linux")]

use std::{fmt::Debug, fs, io, path::PathBuf, process::Command, str::FromStr};

use assertables::assert_ok;
use sakai::{
  Bytes, Cgroup, Error, MaxOr, Pressure,
  error::{ParseError, ParseValueError},
  v2::{
    CgroupPath,
    core::{CgroupController, CgroupType},
    cpu::{CpuIdle, CpuMax, CpuMaxBurst, CpuStat, CpuStatLocal, CpuUclampMax, CpuUclampMin, CpuWeight, Nice},
    memory::{
      MemoryCurrent, MemoryEvents, MemoryEventsLocal, MemoryHigh, MemoryLow, MemoryMax, MemoryMin, MemoryNumaStat,
      MemoryOomGroup, MemoryPeak, MemoryStat, SwapCurrent, SwapEvents, SwapHigh, SwapMax, SwapPeak, ZswapCurrent,
      ZswapMax, ZswapWriteback,
    },
  },
};
use uom::si::information::byte;

#[test]
fn reads_current_cgroup_without_privileges() {
  let cgroup = match Cgroup::from_current_process() {
    | Ok(cgroup) => cgroup,
    | Err(Error::NotCgroupV2) => return,
    | Err(error) => panic!("cgroup discovery failed: {error}"),
  };
  assert_ok!(cgroup.cpu().stat());
  assert_ok!(cgroup.core().controllers());
  assert_ok!(cgroup.core().subtree_control());
  for result in [
    cgroup.cpu().stat_local().map(|_| ()),
    cgroup.cpu().weight().map(|_| ()),
    cgroup.cpu().weight_nice().map(|_| ()),
    cgroup.cpu().max().map(|_| ()),
    cgroup.cpu().max_burst().map(|_| ()),
    cgroup.cpu().pressure().map(|_| ()),
    cgroup.cpu().uclamp_min().map(|_| ()),
    cgroup.cpu().uclamp_max().map(|_| ()),
    cgroup.cpu().idle().map(|_| ()),
    cgroup.core().kind().map(|_| ()),
    cgroup.memory().current().map(|_| ()),
    cgroup.memory().max().map(|_| ()),
    cgroup.memory().high().map(|_| ()),
    cgroup.memory().low().map(|_| ()),
    cgroup.memory().min().map(|_| ()),
    cgroup.memory().peak().map(|_| ()),
    cgroup.memory().stat().map(|_| ()),
    cgroup.memory().events().map(|_| ()),
    cgroup.memory().events_local().map(|_| ()),
    cgroup.memory().oom_group().map(|_| ()),
    cgroup.memory().numa_stat().map(|_| ()),
    cgroup.memory().pressure().map(|_| ()),
    cgroup.memory().swap().current().map(|_| ()),
    cgroup.memory().swap().peak().map(|_| ()),
    cgroup.memory().swap().max().map(|_| ()),
    cgroup.memory().swap().high().map(|_| ()),
    cgroup.memory().swap().events().map(|_| ()),
    cgroup.memory().zswap().current().map(|_| ()),
    cgroup.memory().zswap().max().map(|_| ()),
    cgroup.memory().zswap().writeback().map(|_| ()),
  ] {
    match result {
      | Ok(()) | Err(Error::FileMissing { .. }) => {},
      | Err(error) => panic!("cgroup read failed: {error}"),
    }
  }
  assert_ok!(Cgroup::from_pid(std::process::id()));
  assert_ok!(cgroup.children());
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
  /// Creates a child in vmtest's fresh, disposable cgroup2 hierarchy.
  fn create() -> io::Result<Self> {
    let cgroup = CgroupPath::current().map_err(io::Error::other)?;
    match cgroup.is_read_only() {
      | true => {
        let status = Command::new("mount").args(["-o", "remount,rw"]).arg(cgroup.mount_point()).status()?;
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

    let has_memory_controller = fs::read_to_string(parent.join(CgroupController::CONTROLLERS_FILE_NAME))?
      .split_ascii_whitespace()
      .any(|controller| controller == "memory");
    fs::write(parent.join(CgroupController::SUBTREE_CONTROL_FILE_NAME), "+cpu")?;
    if has_memory_controller {
      fs::write(parent.join(CgroupController::SUBTREE_CONTROL_FILE_NAME), "+memory")?;
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
  let child_path = fixture.path.join("cpu.future");
  assert_ok!(fs::create_dir(&child_path));
  let child = assert_ok!(reader.child(std::ffi::OsStr::new("cpu.future")));
  assert!(assert_ok!(reader.children()).iter().any(|entry| entry.path() == child_path));
  assert_ok!(child.cpu().stat());
  assert_ok!(fs::remove_dir(&child_path));
  let finite = assert_ok!(reader.cpu().max());
  assert_eq!(finite, assert_ok!("25000 100000".parse::<CpuMax>()));
  assert_eq!(assert_ok!(fs::read_to_string(fixture.path.join(CpuMax::FILE_NAME))).trim(), "25000 100000");
  assert_ok!(fs::write(fixture.path.join(CpuMax::FILE_NAME), "max 100000"));
  assert_eq!(assert_ok!(reader.cpu().max()), assert_ok!("max 100000".parse::<CpuMax>()));
  assert_ok!(reader.cpu().stat());
  match reader.cpu().stat_local() {
    | Ok(_) | Err(Error::FileMissing { .. }) => {},
    | Err(error) => panic!("local CPU read failed: {error}"),
  }

  fixture.check(CpuIdle::FILE_NAME, true, reader.cpu().idle());
  fixture.check(CpuMax::FILE_NAME, false, reader.cpu().max());
  fixture.check(CpuMaxBurst::FILE_NAME, true, reader.cpu().max_burst());
  fixture.check(Pressure::CPU_FILE_NAME, false, reader.cpu().pressure());
  fixture.check(CpuStat::FILE_NAME, false, reader.cpu().stat());
  fixture.check(CpuStatLocal::FILE_NAME, true, reader.cpu().stat_local());
  fixture.check(CpuUclampMax::FILE_NAME, true, reader.cpu().uclamp_max());
  fixture.check(CpuUclampMin::FILE_NAME, true, reader.cpu().uclamp_min());
  fixture.check(CpuWeight::FILE_NAME, false, reader.cpu().weight());
  fixture.check(Nice::FILE_NAME, false, reader.cpu().weight_nice());
  assert!(
    assert_ok!(reader.core().controllers()).contains(&CgroupController::Memory),
    "vmtest kernel must provide the memory controller"
  );
  fixture.check(MemoryCurrent::FILE_NAME, false, reader.memory().current());
  fixture.check(MemoryMax::FILE_NAME, false, reader.memory().max());
  fixture.check(MemoryHigh::FILE_NAME, false, reader.memory().high());
  fixture.check(MemoryLow::FILE_NAME, false, reader.memory().low());
  fixture.check(MemoryMin::FILE_NAME, false, reader.memory().min());
  fixture.check(MemoryPeak::FILE_NAME, true, reader.memory().peak());
  fixture.check(MemoryStat::FILE_NAME, false, reader.memory().stat());
  fixture.check(MemoryEvents::FILE_NAME, false, reader.memory().events());
  fixture.check(MemoryEventsLocal::FILE_NAME, true, reader.memory().events_local());
  fixture.check(MemoryOomGroup::FILE_NAME, false, reader.memory().oom_group());
  fixture.check(MemoryNumaStat::FILE_NAME, true, reader.memory().numa_stat());
  fixture.check(Pressure::MEMORY_FILE_NAME, true, reader.memory().pressure());
  fixture.check(SwapCurrent::FILE_NAME, true, reader.memory().swap().current());
  fixture.check(SwapPeak::FILE_NAME, true, reader.memory().swap().peak());
  fixture.check(SwapMax::FILE_NAME, true, reader.memory().swap().max());
  fixture.check(SwapHigh::FILE_NAME, true, reader.memory().swap().high());
  fixture.check(SwapEvents::FILE_NAME, true, reader.memory().swap().events());
  fixture.check(ZswapCurrent::FILE_NAME, true, reader.memory().zswap().current());
  fixture.check(ZswapMax::FILE_NAME, true, reader.memory().zswap().max());
  fixture.check(ZswapWriteback::FILE_NAME, true, reader.memory().zswap().writeback());

  // No process joins the fixture, so its usage is stable across these reads.
  let current_contents = assert_ok!(fs::read_to_string(fixture.path.join(MemoryCurrent::FILE_NAME)));
  let current_bytes = assert_ok!(current_contents.trim().parse::<u64>());
  assert_eq!(assert_ok!(reader.memory().current()).value().get::<byte>(), current_bytes);

  let max_path = fixture.path.join(MemoryMax::FILE_NAME);
  let high_path = fixture.path.join(MemoryHigh::FILE_NAME);
  let low_path = fixture.path.join(MemoryLow::FILE_NAME);
  let min_path = fixture.path.join(MemoryMin::FILE_NAME);
  let oom_group_path = fixture.path.join(MemoryOomGroup::FILE_NAME);
  assert_eq!(assert_ok!(reader.memory().max()).value(), MaxOr::Max);
  assert_eq!(assert_ok!(reader.memory().high()).value(), MaxOr::Max);
  assert_eq!(assert_ok!(reader.memory().low()).value(), Bytes::new::<byte>(0));
  assert_eq!(assert_ok!(reader.memory().min()).value(), Bytes::new::<byte>(0));
  assert!(!assert_ok!(reader.memory().oom_group()).value());
  assert_ok!(fs::write(&oom_group_path, "1"));
  assert!(assert_ok!(reader.memory().oom_group()).value());
  assert_ok!(fs::write(&oom_group_path, "0"));
  assert!(!assert_ok!(reader.memory().oom_group()).value());
  assert_ok!(fs::write(&max_path, "67108864"));
  assert_ok!(fs::write(&high_path, "33554432"));
  assert_ok!(fs::write(&low_path, "16777216"));
  assert_ok!(fs::write(&min_path, "8388608"));
  assert_eq!(assert_ok!(fs::read_to_string(&max_path)).trim(), "67108864");
  assert_eq!(assert_ok!(fs::read_to_string(&high_path)).trim(), "33554432");
  assert_eq!(assert_ok!(fs::read_to_string(&low_path)).trim(), "16777216");
  assert_eq!(assert_ok!(fs::read_to_string(&min_path)).trim(), "8388608");
  assert_eq!(assert_ok!(reader.memory().max()).value(), MaxOr::Value(Bytes::new::<byte>(67_108_864)));
  assert_eq!(assert_ok!(reader.memory().high()).value(), MaxOr::Value(Bytes::new::<byte>(33_554_432)));
  assert_eq!(assert_ok!(reader.memory().low()).value(), Bytes::new::<byte>(16_777_216));
  assert_eq!(assert_ok!(reader.memory().min()).value(), Bytes::new::<byte>(8_388_608));
  assert_eq!(assert_ok!(reader.core().kind()), CgroupType::Domain);
  assert_ok!(reader.core().controllers());
  assert_ok!(reader.core().subtree_control());
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
    (root.memory().current().map(|_| ()), MemoryCurrent::FILE_NAME),
    (root.memory().max().map(|_| ()), MemoryMax::FILE_NAME),
    (root.memory().high().map(|_| ()), MemoryHigh::FILE_NAME),
    (root.memory().low().map(|_| ()), MemoryLow::FILE_NAME),
    (root.memory().min().map(|_| ()), MemoryMin::FILE_NAME),
    (root.memory().peak().map(|_| ()), MemoryPeak::FILE_NAME),
    (root.memory().events().map(|_| ()), MemoryEvents::FILE_NAME),
    (root.memory().events_local().map(|_| ()), MemoryEventsLocal::FILE_NAME),
    (root.memory().oom_group().map(|_| ()), MemoryOomGroup::FILE_NAME),
    (root.memory().swap().current().map(|_| ()), SwapCurrent::FILE_NAME),
    (root.memory().swap().peak().map(|_| ()), SwapPeak::FILE_NAME),
    (root.memory().swap().max().map(|_| ()), SwapMax::FILE_NAME),
    (root.memory().swap().high().map(|_| ()), SwapHigh::FILE_NAME),
    (root.memory().swap().events().map(|_| ()), SwapEvents::FILE_NAME),
    (root.memory().zswap().current().map(|_| ()), ZswapCurrent::FILE_NAME),
    (root.memory().zswap().max().map(|_| ()), ZswapMax::FILE_NAME),
  ] {
    assert!(matches!(result, Err(Error::FileMissing { path }) if path == root_path.join(name)));
  }
  assert_ok!(root.memory().stat());
  for (name, read) in [
    (MemoryNumaStat::FILE_NAME, root.memory().numa_stat().map(|_| ())),
    (ZswapWriteback::FILE_NAME, root.memory().zswap().writeback().map(|_| ())),
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
