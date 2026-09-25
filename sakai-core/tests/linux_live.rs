#![cfg(target_os = "linux")]

use std::{fmt::Debug, fs, io, path::PathBuf, process::Command, str::FromStr};

use assertables::assert_ok;
use sakai_core::cgroup::v2::{
  CgroupPath,
  cpu::{
    CpuMax, CpuMaxBurst, CpuStat, CpuUclampMax, CpuUclampMin, CpuWeight,
    Pressure,
  },
};

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
  assert_eq!(
    assert_ok!(fs::read_to_string(fixture.path.join("cpu.max"))).trim(),
    "25000 100000"
  );
  assert_ok!(fs::write(fixture.path.join("cpu.max"), "max 100000"));

  fixture.check::<CpuMax>("cpu.max", false);
  fixture.check::<CpuMaxBurst>("cpu.max.burst", true);
  fixture.check::<Pressure>("cpu.pressure", false);
  fixture.check::<CpuStat>("cpu.stat", false);
  fixture.check::<CpuUclampMax>("cpu.uclamp.max", true);
  fixture.check::<CpuUclampMin>("cpu.uclamp.min", true);
  fixture.check::<CpuWeight>("cpu.weight", false);
}
