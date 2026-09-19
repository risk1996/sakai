use std::str::FromStr;

use crate::cgroup::common::{
  error::ParseError,
  parser::{ParsePercent, ParseValueError, Parser},
  unit::{MaxOr, Ratio},
};

/// The minimum CPU utilization requested by a cgroup's `cpu.uclamp.min` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value. The kernel reports utilization as a decimal
/// percentage, which is exposed as a dimensionless [`Ratio`]. This request is
/// a performance hint rather than a CPU bandwidth guarantee.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuUclampMin {
  value: Ratio,
}

impl CpuUclampMin {
  /// Returns the requested minimum CPU utilization.
  #[must_use]
  pub const fn value(self) -> Ratio {
    self.value
  }
}

impl FromStr for CpuUclampMin {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParsePercent, _>("utilization")?,
      })
    })
  }
}

/// The maximum CPU utilization requested by a cgroup's `cpu.uclamp.max` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain a fresh value. A concrete limit is reported as a decimal
/// percentage and exposed as a dimensionless [`Ratio`]. [`MaxOr::Max`] means
/// that the cgroup requests no utilization cap. This request is a performance
/// hint rather than a CPU bandwidth limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuUclampMax {
  value: MaxOr<Ratio>,
}

impl CpuUclampMax {
  /// Returns the requested maximum CPU utilization.
  #[must_use]
  pub const fn value(self) -> MaxOr<Ratio> {
    self.value
  }
}

impl FromStr for CpuUclampMax {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    Parser::parse(contents, |parser| {
      Ok(Self {
        value: parser.next_field::<ParsePercent, _>("utilization")?,
      })
    })
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use uom::si::ratio::percent;

  use super::*;

  #[test]
  fn parses_cpu_uclamp() {
    struct TestCase {
      input: &'static str,
      expected_min: Result<CpuUclampMin, &'static str>,
      expected_max: Result<CpuUclampMax, &'static str>,
    }

    let missing = "cgroup content \"\" has missing field \"utilization\"";
    let excess =
      "cgroup content \"12.34 extra\" has excess field \"additional\"";
    let invalid_max = "cgroup content \"max\" has an invalid field \
                       \"utilization\" value \"max\"";
    let invalid_unlimited = "cgroup content \"unlimited\" has an invalid \
                             field \"utilization\" value \"unlimited\"";
    let invalid_negative = "cgroup content \"-0.01\" has an invalid field \
                            \"utilization\" value \"-0.01\"";
    let invalid_excess = "cgroup content \"100.01\" has an invalid field \
                          \"utilization\" value \"100.01\"";
    let invalid_nan = "cgroup content \"NaN\" has an invalid field \
                       \"utilization\" value \"NaN\"";
    let cases = [
      TestCase {
        input: "0.00\n",
        expected_min: Ok(CpuUclampMin {
          value: Ratio::new::<percent>(0.00),
        }),
        expected_max: Ok(CpuUclampMax {
          value: MaxOr::Value(Ratio::new::<percent>(0.00)),
        }),
      },
      TestCase {
        input: "12.34\n",
        expected_min: Ok(CpuUclampMin {
          value: Ratio::new::<percent>(12.34),
        }),
        expected_max: Ok(CpuUclampMax {
          value: MaxOr::Value(Ratio::new::<percent>(12.34)),
        }),
      },
      TestCase {
        input: "98.76\n",
        expected_min: Ok(CpuUclampMin {
          value: Ratio::new::<percent>(98.76),
        }),
        expected_max: Ok(CpuUclampMax {
          value: MaxOr::Value(Ratio::new::<percent>(98.76)),
        }),
      },
      TestCase {
        input: "100.00",
        expected_min: Ok(CpuUclampMin {
          value: Ratio::new::<percent>(100.00),
        }),
        expected_max: Ok(CpuUclampMax {
          value: MaxOr::Value(Ratio::new::<percent>(100.00)),
        }),
      },
      TestCase {
        input: "max",
        expected_min: Err(invalid_max),
        expected_max: Ok(CpuUclampMax { value: MaxOr::Max }),
      },
      TestCase {
        input: "",
        expected_min: Err(missing),
        expected_max: Err(missing),
      },
      TestCase {
        input: "12.34 extra",
        expected_min: Err(excess),
        expected_max: Err(excess),
      },
      TestCase {
        input: "unlimited",
        expected_min: Err(invalid_unlimited),
        expected_max: Err(invalid_unlimited),
      },
      TestCase {
        input: "-0.01",
        expected_min: Err(invalid_negative),
        expected_max: Err(invalid_negative),
      },
      TestCase {
        input: "100.01",
        expected_min: Err(invalid_excess),
        expected_max: Err(invalid_excess),
      },
      TestCase {
        input: "NaN",
        expected_min: Err(invalid_nan),
        expected_max: Err(invalid_nan),
      },
    ];

    for case in cases {
      let actual_min = case.input.parse::<CpuUclampMin>();
      match case.expected_min {
        | Ok(expected) => {
          let actual = assert_ok!(actual_min, "input: {:?}", case.input);
          assert_eq!(actual, expected, "input: {:?}", case.input);
        },
        | Err(message) => {
          let actual = assert_err!(actual_min, "input: {:?}", case.input);
          assert_eq!(actual.to_string(), message, "input: {:?}", case.input);
        },
      }

      let actual_max = case.input.parse::<CpuUclampMax>();
      match case.expected_max {
        | Ok(expected) => {
          let actual = assert_ok!(actual_max, "input: {:?}", case.input);
          assert_eq!(actual, expected, "input: {:?}", case.input);
        },
        | Err(message) => {
          let actual = assert_err!(actual_max, "input: {:?}", case.input);
          assert_eq!(actual.to_string(), message, "input: {:?}", case.input);
        },
      }
    }
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn parses_live_root_cpu_uclamp_when_available() {
    for (path, parse) in [
      ("/sys/fs/cgroup/cpu.uclamp.min", |contents: &str| {
        contents.parse::<CpuUclampMin>().map(|_| ())
      }),
      ("/sys/fs/cgroup/cpu.uclamp.max", |contents: &str| {
        contents.parse::<CpuUclampMax>().map(|_| ())
      }),
    ] {
      let contents = match std::fs::read_to_string(path) {
        | Ok(contents) => contents,
        | Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
          continue;
        },
        | Err(error) => panic!("failed to read {path}: {error}"),
      };

      assert_ok!(parse(&contents));
    }
  }
}
