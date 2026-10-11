#![cfg(target_os = "linux")]

use std::{fmt::Debug, fs, io, path::PathBuf, process::Command, str::FromStr, time::Duration};

use assertables::{assert_ok, assert_some};
use sakai::{
  Bytes, Cgroup, Error, MaxOr, Pressure,
  error::{ParseError, ParseValueError},
  v2::{
    CgroupPath,
    core::{CgroupController, CgroupEvents, CgroupStat, CgroupType},
    cpu::{CpuIdle, CpuMax, CpuMaxBurst, CpuStat, CpuStatLocal, CpuUclampMax, CpuUclampMin, CpuWeight, Nice},
    cpuset::{CpuList, CpusetPartition, MemoryNodeList},
    io::{IoLatency, IoMax, IoStat, IoWeight},
    memory::{
      MemoryCurrent, MemoryEvents, MemoryEventsLocal, MemoryHigh, MemoryLow, MemoryMax, MemoryMin, MemoryNumaStat,
      MemoryOomGroup, MemoryPeak, MemoryStat, SwapCurrent, SwapEvents, SwapHigh, SwapMax, SwapPeak, ZswapCurrent,
      ZswapMax, ZswapWriteback,
    },
    notification::{ConfigurationInterface, EventInterface},
    pids::{PidsCurrent, PidsEvents, PidsMax},
  },
};
use strum::IntoEnumIterator;
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
  assert_ok!(cgroup.core().stat());
  for result in [
    cgroup.cpuset().cpus().map(|_| ()),
    cgroup.cpuset().cpus_effective().map(|_| ()),
    cgroup.cpuset().mems().map(|_| ()),
    cgroup.cpuset().mems_effective().map(|_| ()),
    cgroup.cpuset().partition().map(|_| ()),
    cgroup.io().stat().map(|_| ()),
    cgroup.io().pressure().map(|_| ()),
    cgroup.io().weight().map(|_| ()),
    cgroup.io().max().map(|_| ()),
    cgroup.io().latency().map(|_| ()),
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
    cgroup.core().events().map(|_| ()),
    cgroup.pids().current().map(|_| ()),
    cgroup.pids().max().map(|_| ()),
    cgroup.pids().events().map(|_| ()),
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
  let controllers = assert_ok!(reader.core().controllers());
  if controllers.contains(&CgroupController::Rdma) {
    assert_ok!(reader.rdma().current());
    assert_ok!(reader.rdma().max());
  }
  if controllers.contains(&CgroupController::Dmem) {
    assert_ok!(reader.dmem().current());
    assert_ok!(reader.dmem().min());
    assert_ok!(reader.dmem().low());
    assert_ok!(reader.dmem().max());
  }
  if controllers.contains(&CgroupController::Misc) {
    assert_ok!(reader.misc().current());
    assert_ok!(reader.misc().max());
  }
  if controllers.contains(&CgroupController::Hugetlb) {
    let size = assert_ok!("2MB".parse::<sakai::v2::hugetlb::PageSize>());
    assert_ok!(reader.hugetlb().current(&size));
    assert_ok!(reader.hugetlb().max(&size));
    assert_ok!(reader.hugetlb().reserved_current(&size));
    assert_ok!(reader.hugetlb().reserved_max(&size));
    assert_ok!(reader.hugetlb().events(&size));
    assert_ok!(reader.hugetlb().events_local(&size));
  }

  let child_path = fixture.path.join("cpu.future");
  assert_ok!(fs::create_dir(&child_path));
  let child = assert_ok!(reader.child(std::ffi::OsStr::new("cpu.future")));
  assert!(assert_ok!(reader.children()).iter().any(|entry| entry.path() == child_path));
  assert_ok!(child.cpu().stat());
  assert_ok!(fs::remove_dir(&child_path));
  let finite = assert_ok!(reader.cpu().max());
  assert_eq!(finite, assert_ok!("25000 100000".parse::<CpuMax>()));
  assert_eq!(assert_ok!(fs::read_to_string(fixture.path.join(CpuMax::FILE_NAME))).trim(), "25000 100000");
  fixture.configure(CpuMax::FILE_NAME, "max 100000", assert_ok!("max 100000".parse::<CpuMax>()), || reader.cpu().max());
  assert_ok!(reader.cpu().stat());
  match reader.cpu().stat_local() {
    | Ok(_) | Err(Error::FileMissing { .. }) => {},
    | Err(error) => panic!("local CPU read failed: {error}"),
  }

  match assert_ok!(reader.core().controllers()).contains(&CgroupController::Cpuset) {
    | true => {
      fixture.check(CpuList::CPUS_FILE_NAME, false, reader.cpuset().cpus());
      fixture.check(CpuList::EFFECTIVE_FILE_NAME, false, reader.cpuset().cpus_effective());
      fixture.check(MemoryNodeList::MEMS_FILE_NAME, false, reader.cpuset().mems());
      fixture.check(MemoryNodeList::EFFECTIVE_FILE_NAME, false, reader.cpuset().mems_effective());
      fixture.check(CpusetPartition::FILE_NAME, true, reader.cpuset().partition());
      assert_eq!(assert_ok!(reader.cpuset().cpus()), CpuList::default());
      assert_eq!(assert_ok!(reader.cpuset().mems()), MemoryNodeList::default());
      // Configure a granted resource, then check a fresh read and unchanged file.
      // No process enters this fixture; these writes only exercise the test hierarchy.
      let cpu = assert_ok!(reader.cpuset().cpus_effective()).ids().next().expect("online CPU").to_string();
      let node = assert_ok!(reader.cpuset().mems_effective()).ids().next().expect("online memory node").to_string();
      fixture.configure(CpuList::CPUS_FILE_NAME, &cpu, assert_ok!(cpu.parse::<CpuList>()), || reader.cpuset().cpus());
      fixture.configure(MemoryNodeList::MEMS_FILE_NAME, &node, assert_ok!(node.parse::<MemoryNodeList>()), || {
        reader.cpuset().mems()
      });
    },
    | false => {},
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
  fixture.check(PidsCurrent::FILE_NAME, false, reader.pids().current());
  fixture.check(PidsMax::FILE_NAME, false, reader.pids().max());
  fixture.check(PidsEvents::FILE_NAME, false, reader.pids().events());
  fixture.check(CgroupEvents::FILE_NAME, false, reader.core().events());
  fixture.check(CgroupStat::FILE_NAME, false, reader.core().stat());
  assert!(
    assert_ok!(reader.core().controllers()).contains(&CgroupController::Io),
    "vmtest kernel must provide the I/O controller"
  );
  fixture.check(IoStat::FILE_NAME, false, reader.io().stat());
  fixture.check(Pressure::IO_FILE_NAME, true, reader.io().pressure());
  fixture.check(IoWeight::FILE_NAME, true, reader.io().weight());
  fixture.check(IoMax::FILE_NAME, false, reader.io().max());
  fixture.check(IoLatency::FILE_NAME, true, reader.io().latency());
  assert_eq!(assert_ok!(reader.io().stat()), IoStat::default());
  assert_eq!(assert_ok!(reader.io().max()), IoMax::default());
  let weight_path = fixture.path.join(IoWeight::FILE_NAME);
  match fs::read_to_string(&weight_path) {
    | Ok(_) => {
      for weight in [1, 100, 10_000] {
        let configuration = format!("default {weight}");
        fixture.configure(IoWeight::FILE_NAME, &configuration, assert_ok!(configuration.parse::<IoWeight>()), || {
          reader.io().weight()
        });
      }
    },
    | Err(error) if error.kind() == io::ErrorKind::NotFound => {},
    | Err(error) => panic!("failed to read {weight_path:?}: {error}"),
  }
  assert_eq!(assert_ok!(reader.pids().current()).value().value, 0);
  assert_eq!(assert_ok!(reader.pids().max()).value(), MaxOr::Max);
  for limit in ["0", "10", "max"] {
    fixture.configure(PidsMax::FILE_NAME, limit, assert_ok!(limit.parse::<PidsMax>()), || reader.pids().max());
  }
  assert_eq!(assert_ok!(reader.pids().events()).max().value, 0);
  let events = assert_ok!(reader.core().events());
  assert!(!events.populated());
  assert_eq!(events.frozen(), Some(false));
  assert_eq!(assert_ok!(reader.core().stat()).descendants().value, 0);
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

  assert_eq!(assert_ok!(reader.memory().max()).value(), MaxOr::Max);
  assert_eq!(assert_ok!(reader.memory().high()).value(), MaxOr::Max);
  assert_eq!(assert_ok!(reader.memory().low()).value(), Bytes::new::<byte>(0));
  assert_eq!(assert_ok!(reader.memory().min()).value(), Bytes::new::<byte>(0));
  assert!(!assert_ok!(reader.memory().oom_group()).value());
  for (input, expected) in [("1", true), ("0", false)] {
    fixture
      .configure(MemoryOomGroup::FILE_NAME, input, expected, || reader.memory().oom_group().map(|value| value.value()));
  }
  fixture.configure(MemoryMax::FILE_NAME, "67108864", MaxOr::Value(Bytes::new::<byte>(67_108_864)), || {
    reader.memory().max().map(|value| value.value())
  });
  fixture.configure(MemoryHigh::FILE_NAME, "33554432", MaxOr::Value(Bytes::new::<byte>(33_554_432)), || {
    reader.memory().high().map(|value| value.value())
  });
  fixture.configure(MemoryLow::FILE_NAME, "16777216", Bytes::new::<byte>(16_777_216), || {
    reader.memory().low().map(|value| value.value())
  });
  fixture.configure(MemoryMin::FILE_NAME, "8388608", Bytes::new::<byte>(8_388_608), || {
    reader.memory().min().map(|value| value.value())
  });
  assert_eq!(assert_ok!(reader.core().kind()), CgroupType::Domain);
  assert_ok!(reader.core().controllers());
  assert_ok!(reader.core().subtree_control());

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
    assert_eq!(assert_ok!(notification_group.core().events()).frozen(), Some(frozen));
    assert!(!assert_ok!(events.wait(Some(Duration::ZERO))));
  }
  for contents in ["25000 100000", "max 100000"] {
    assert_ok!(fs::write(notification_path.join(CpuMax::FILE_NAME), contents));
    let change = assert_ok!(configuration.wait(notification_timeout)).expect("configuration notification");
    assert!(change.modified());
    assert!(!change.removed());
    assert!(!change.overflowed());
    assert_eq!(assert_ok!(notification_group.cpu().max()), assert_ok!(contents.parse::<CpuMax>()));
    assert_eq!(assert_ok!(fs::read_to_string(notification_path.join(CpuMax::FILE_NAME))).trim(), contents);
    assert_eq!(assert_ok!(configuration.wait(Some(Duration::ZERO))), None);
  }
  drop(events);
  drop(notification_group);
  assert_ok!(fs::remove_dir(&notification_path));
  // Kernfs internal removal need not emit an inotify deletion event. A
  // notification, when supplied by the kernel, must describe watch removal.
  match assert_ok!(configuration.wait(Some(Duration::ZERO))) {
    | Some(change) => assert!(change.removed()),
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
    (IoWeight::FILE_NAME, root.io().weight().map(|_| ())),
    (IoMax::FILE_NAME, root.io().max().map(|_| ())),
    (IoLatency::FILE_NAME, root.io().latency().map(|_| ())),
  ] {
    assert!(matches!(result, Err(Error::FileMissing { path: missing }) if missing == path.join(name)));
  }
  // Older kernels omit root io.stat; PSI can be disabled at build/boot time.
  for (name, result) in
    [(IoStat::FILE_NAME, root.io().stat().map(|_| ())), (Pressure::IO_FILE_NAME, root.io().pressure().map(|_| ()))]
  {
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
  assert_ok!(root.core().stat());
  for (name, read) in [
    (CgroupEvents::FILE_NAME, root.core().events().map(|_| ())),
    (PidsMax::FILE_NAME, root.pids().max().map(|_| ())),
    (PidsEvents::FILE_NAME, root.pids().events().map(|_| ())),
  ] {
    assert!(matches!(read, Err(Error::FileMissing { path }) if path == root_path.join(name)));
  }
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

#[test]
fn reads_root_placement_and_reports_configuration_exemptions() {
  match std::env::var_os("SAKAI_VMTEST") {
    | None => return,
    | Some(_) => {},
  }
  let path = PathBuf::from("/sys/fs/cgroup");
  let root = assert_ok!(Cgroup::from_path(&path));
  match assert_ok!(root.core().controllers()).contains(&CgroupController::Cpuset) {
    | true => {
      assert_ok!(root.cpuset().cpus_effective());
      assert_ok!(root.cpuset().mems_effective());
      for (name, result) in [
        (CpuList::CPUS_FILE_NAME, root.cpuset().cpus().map(|_| ())),
        (MemoryNodeList::MEMS_FILE_NAME, root.cpuset().mems().map(|_| ())),
        (CpusetPartition::FILE_NAME, root.cpuset().partition().map(|_| ())),
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
  let root = assert_ok!(Cgroup::from_path(std::path::Path::new("/sys/fs/cgroup")));
  assert_ok!(check_remaining_controllers(&root));
}

fn check_remaining_controllers(group: &Cgroup) -> io::Result<()> {
  for (name, result) in [
    ("misc.capacity", group.misc().capacity().map(|_| ())),
    ("misc.current", group.misc().current().map(|_| ())),
    ("misc.peak", group.misc().peak().map(|_| ())),
    ("misc.max", group.misc().max().map(|_| ())),
    ("misc.events", group.misc().events().map(|_| ())),
    ("misc.events.local", group.misc().events_local().map(|_| ())),
    ("dmem.capacity", group.dmem().capacity().map(|_| ())),
    ("dmem.current", group.dmem().current().map(|_| ())),
    ("dmem.min", group.dmem().min().map(|_| ())),
    ("dmem.low", group.dmem().low().map(|_| ())),
    ("dmem.max", group.dmem().max().map(|_| ())),
    ("rdma.current", group.rdma().current().map(|_| ())),
    ("rdma.peak", group.rdma().peak().map(|_| ())),
    ("rdma.max", group.rdma().max().map(|_| ())),
    ("rdma.events", group.rdma().events().map(|_| ())),
    ("rdma.events.local", group.rdma().events_local().map(|_| ())),
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
    let size = assert_ok!(sakai::v2::hugetlb::PageSize::try_new(Bytes::new::<byte>(bytes)));
    let reader = group.hugetlb();
    for (suffix, result) in [
      ("current", reader.current(&size).map(|_| ())),
      ("max", reader.max(&size).map(|_| ())),
      ("rsvd.current", reader.reserved_current(&size).map(|_| ())),
      ("rsvd.max", reader.reserved_max(&size).map(|_| ())),
      ("events", reader.events(&size).map(|_| ())),
      ("events.local", reader.events_local(&size).map(|_| ())),
      ("numa_stat", reader.numa_stat(&size).map(|_| ())),
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
