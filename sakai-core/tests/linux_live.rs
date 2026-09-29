#![cfg(target_os = "linux")]

use std::{fmt::Debug, fs, io, path::PathBuf, process::Command, str::FromStr};

use assertables::assert_ok;
use sakai_core::{
  Cgroup, Error, Pressure,
  error::{ParseError, ParseValueError},
  v2::{
    CgroupPath,
    core::ReadCore,
    cpu::{
      CpuIdle, CpuMax, CpuMaxBurst, CpuStat, CpuStatLocal, CpuUclampMax,
      CpuUclampMin, CpuWeight, Nice, ReadCpu,
    },
  },
};

#[test]
fn reads_current_cpu_without_privileges() {
  let cgroup = match Cgroup::from_current_process() {
    | Ok(cgroup) => cgroup,
    | Err(Error::NotCgroupV2) => return,
    | Err(error) => panic!("cgroup discovery failed: {error}"),
  };
  assert_ok!(cgroup.stat());
  assert_ok!(cgroup.controllers());
  assert_ok!(cgroup.subtree_control());
  for result in [
    cgroup.stat_local().map(|_| ()),
    cgroup.weight().map(|_| ()),
    cgroup.weight_nice().map(|_| ()),
    cgroup.max().map(|_| ()),
    cgroup.max_burst().map(|_| ()),
    cgroup.pressure().map(|_| ()),
    cgroup.uclamp_min().map(|_| ()),
    cgroup.uclamp_max().map(|_| ()),
    cgroup.idle().map(|_| ()),
    cgroup.r#type().map(|_| ()),
  ] {
    match result {
      | Ok(()) | Err(Error::FileMissing { .. }) => {},
      | Err(error) => panic!("CPU read failed: {error}"),
    }
  }
  assert_ok!(Cgroup::from_pid(std::process::id()));
  assert_ok!(cgroup.children());
}

#[test]
fn rejects_non_cgroup_filesystems() {
  assert!(matches!(
    Cgroup::from_path(std::path::Path::new("/")),
    Err(Error::NotCgroupV2)
  ));
}

#[test]
fn parses_available_root_cpu_interfaces() {
  type Parse = fn(&str) -> Result<(), ParseError<ParseValueError>>;

  let parsers: [(&str, Parse); 10] = [
    ("cpu.idle", |contents| {
      contents.parse::<CpuIdle>().map(|_| ())
    }),
    ("cpu.max", |contents| contents.parse::<CpuMax>().map(|_| ())),
    ("cpu.max.burst", |contents| {
      contents.parse::<CpuMaxBurst>().map(|_| ())
    }),
    ("cpu.pressure", |contents| {
      contents.parse::<Pressure>().map(|_| ())
    }),
    ("cpu.stat", |contents| {
      contents.parse::<CpuStat>().map(|_| ())
    }),
    ("cpu.stat.local", |contents| {
      contents.parse::<CpuStatLocal>().map(|_| ())
    }),
    ("cpu.uclamp.max", |contents| {
      contents.parse::<CpuUclampMax>().map(|_| ())
    }),
    ("cpu.uclamp.min", |contents| {
      contents.parse::<CpuUclampMin>().map(|_| ())
    }),
    ("cpu.weight", |contents| {
      contents.parse::<CpuWeight>().map(|_| ())
    }),
    ("cpu.weight.nice", |contents| {
      contents.parse::<Nice>().map(|_| ())
    }),
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
  fn create() -> io::Result<Self> {
    let cgroup = CgroupPath::current().map_err(io::Error::other)?;
    match cgroup.is_read_only() {
      | true => {
        let status = Command::new("mount")
          .args(["-o", "remount,rw"])
          .arg(cgroup.mount_point())
          .status()?;
        match status.success() {
          | true => {},
          | false => {
            return Err(io::Error::other(format!(
              "cgroup remount failed: {status}"
            )));
          },
        }
      },
      | false => {},
    }
    let parent = cgroup.as_ref();
    let path = parent.join(format!("sakai-vmtest-{}", std::process::id()));

    fs::write(parent.join("cgroup.subtree_control"), "+cpu")?;
    fs::create_dir(&path)?;
    let fixture = Self { path };
    fs::write(fixture.path.join("cpu.max"), "25000 100000")?;

    Ok(fixture)
  }

  fn check<T: FromStr + Debug>(&self, file: &str, optional: bool)
  where
    T::Err: Debug, {
    let path = self.path.join(file);
    let contents = match fs::read_to_string(&path) {
      | Err(error) if optional && error.kind() == io::ErrorKind::NotFound => {
        return;
      },
      | result => assert_ok!(result, "path: {path:?}"),
    };
    assert_ok!(contents.parse::<T>(), "path: {path:?}");
  }
}

impl Drop for CgroupFixture {
  fn drop(&mut self) {
    drop(fs::remove_dir(&self.path));
  }
}

#[test]
fn parses_live_delegated_cpu_interfaces() {
  match std::env::var_os("SAKAI_VMTEST") {
    | None => return,
    | Some(_) => {},
  }

  let fixture = assert_ok!(CgroupFixture::create());
  let reader = assert_ok!(Cgroup::from_path(&fixture.path));
  let child_path = fixture.path.join("cpu.future");
  assert_ok!(fs::create_dir(&child_path));
  let child = assert_ok!(reader.child(std::ffi::OsStr::new("cpu.future")));
  assert!(
    assert_ok!(reader.children())
      .iter()
      .any(|entry| entry.path() == child_path)
  );
  assert_ok!(child.stat());
  assert_ok!(fs::remove_dir(&child_path));
  let finite = assert_ok!(reader.max());
  assert_eq!(finite, assert_ok!("25000 100000".parse::<CpuMax>()));
  assert_eq!(
    assert_ok!(fs::read_to_string(fixture.path.join("cpu.max"))).trim(),
    "25000 100000"
  );
  assert_ok!(fs::write(fixture.path.join("cpu.max"), "max 100000"));
  assert_eq!(
    assert_ok!(reader.max()),
    assert_ok!("max 100000".parse::<CpuMax>())
  );
  assert_ok!(reader.stat());
  match reader.stat_local() {
    | Ok(_) | Err(Error::FileMissing { .. }) => {},
    | Err(error) => panic!("local CPU read failed: {error}"),
  }

  fixture.check::<CpuIdle>("cpu.idle", true);
  fixture.check::<CpuMax>("cpu.max", false);
  fixture.check::<CpuMaxBurst>("cpu.max.burst", true);
  fixture.check::<Pressure>("cpu.pressure", false);
  fixture.check::<CpuStat>("cpu.stat", false);
  fixture.check::<CpuUclampMax>("cpu.uclamp.max", true);
  fixture.check::<CpuUclampMin>("cpu.uclamp.min", true);
  fixture.check::<CpuWeight>("cpu.weight", false);
  fixture.check::<Nice>("cpu.weight.nice", false);
}
