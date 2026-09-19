use std::{collections::BTreeMap, str::FromStr};

use crate::cgroup::common::{
  error::ParseError,
  parser::{ParseCgroup, ParseMicroseconds, ParsePercent, ParseValueError},
  unit::{Ratio, Time},
};

/// Pressure stall information from a cgroup `*.pressure` file.
///
/// Values are a point-in-time read of the kernel interface. Read the file
/// again to obtain fresh values. Averages are ratios over the preceding 10,
/// 60, and 300 seconds. Total stall times are reported by the kernel in
/// microseconds and stored as [`Time`].
///
/// CPU pressure files on older kernels contain only the `some` line, so
/// [`Self::full`] is optional. Reading pressure never requires writing a PSI
/// trigger to the file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pressure {
  some: PressureLine,
  full: Option<PressureLine>,
}

impl Pressure {
  /// Returns pressure during which at least one task was stalled.
  #[must_use]
  pub const fn some(self) -> PressureLine {
    self.some
  }

  /// Returns pressure during which all non-idle tasks were stalled.
  ///
  /// Older CPU PSI interfaces do not report this line.
  #[must_use]
  pub const fn full(self) -> Option<PressureLine> {
    self.full
  }
}

/// A set of rolling averages and cumulative stall time from PSI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PressureLine {
  avg10: Ratio,
  avg60: Ratio,
  avg300: Ratio,
  total: Time,
}

impl PressureLine {
  /// Returns the average stalled-time ratio over the preceding 10 seconds.
  #[must_use]
  pub const fn avg10(self) -> Ratio {
    self.avg10
  }

  /// Returns the average stalled-time ratio over the preceding 60 seconds.
  #[must_use]
  pub const fn avg60(self) -> Ratio {
    self.avg60
  }

  /// Returns the average stalled-time ratio over the preceding 300 seconds.
  #[must_use]
  pub const fn avg300(self) -> Ratio {
    self.avg300
  }

  /// Returns cumulative stall time.
  #[must_use]
  pub const fn total(self) -> Time {
    self.total
  }

  fn from_fields(
    fields: &PressureFields<'_>,
  ) -> Result<Self, ParseError<'static, ParseValueError>> {
    Ok(Self {
      avg10: fields.required::<ParsePercent, _>(PressureField::Avg10)?,
      avg60: fields.required::<ParsePercent, _>(PressureField::Avg60)?,
      avg300: fields.required::<ParsePercent, _>(PressureField::Avg300)?,
      total: fields.required::<ParseMicroseconds, _>(PressureField::Total)?,
    })
  }
}

impl FromStr for Pressure {
  type Err = ParseError<'static, ParseValueError>;

  fn from_str(contents: &str) -> Result<Self, Self::Err> {
    let lines = contents
      .lines()
      .filter(|line| !line.trim().is_empty())
      .filter_map(|line| PressureFields::parse(contents, line).transpose())
      .map(|result| {
        result.and_then(|(kind, fields)| {
          Ok((kind, PressureLine::from_fields(&fields)?))
        })
      })
      .collect::<Result<BTreeMap<_, _>, _>>()?;

    Ok(Self {
      some: lines
        .get(&PressureKind::Some)
        .copied()
        .ok_or_else(|| ParseError::missing(contents, "some"))?,
      full: lines.get(&PressureKind::Full).copied(),
    })
  }
}

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString,
)]
#[strum(serialize_all = "lowercase")]
enum PressureKind {
  Some,
  Full,
}

#[derive(
  Debug,
  Clone,
  Copy,
  PartialEq,
  Eq,
  PartialOrd,
  Ord,
  Hash,
  strum::EnumString,
  strum::Display,
  strum::IntoStaticStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum PressureField {
  Avg10,
  Avg60,
  Avg300,
  Total,
}

struct PressureFields<'a> {
  raw: &'a str,
  values: BTreeMap<PressureField, &'a str>,
}

impl<'a> PressureFields<'a> {
  fn parse(
    raw: &'a str,
    line: &'a str,
  ) -> Result<Option<(PressureKind, Self)>, ParseError<'static, ParseValueError>>
  {
    let kind = line
      .split_ascii_whitespace()
      .next()
      .ok_or_else(|| ParseError::missing(raw, "kind"))?;
    let Ok(kind) = PressureKind::from_str(kind) else {
      return Ok(None);
    };

    let values = line
      .split_ascii_whitespace()
      .skip(1)
      .filter_map(|token| match token.split_once('=') {
        | Some((key, value)) => PressureField::from_str(key)
          .ok()
          .map(|field| Ok((field, value))),
        | None => Some(Err(ParseError::invalid(
          raw,
          "value",
          token,
          ParseValueError::OutOfRange,
        ))),
      })
      .collect::<Result<_, _>>()?;

    Ok(Some((kind, Self { raw, values })))
  }

  fn required<Unit, T>(
    &self,
    field: PressureField,
  ) -> Result<T, ParseError<'static, T::Error>>
  where
    T: ParseCgroup<Unit>, {
    let name = field.into();
    let value = self
      .values
      .get(&field)
      .ok_or_else(|| ParseError::missing(self.raw, name))?;

    T::parse_field(self.raw, name, value)
  }
}

#[cfg(test)]
mod tests {
  use assertables::{assert_err, assert_ok};
  use indoc::indoc;
  use uom::si::{ratio::percent, time::microsecond};

  use super::*;

  #[test]
  fn parses_cpu_pressure() {
    struct TestCase {
      input: &'static str,
      expected: Result<Pressure, &'static str>,
    }

    let cases = [
      TestCase {
        input: indoc! {"
          some avg10=12.34 avg60=5.67 avg300=0.89 total=1234567
          full avg10=1.25 avg60=0.50 avg300=0.10 total=98765
        "},
        expected: Ok(Pressure {
          some: PressureLine {
            avg10: Ratio::new::<percent>(12.34),
            avg60: Ratio::new::<percent>(5.67),
            avg300: Ratio::new::<percent>(0.89),
            total: Time::new::<microsecond>(1_234_567),
          },
          full: Some(PressureLine {
            avg10: Ratio::new::<percent>(1.25),
            avg60: Ratio::new::<percent>(0.50),
            avg300: Ratio::new::<percent>(0.10),
            total: Time::new::<microsecond>(98_765),
          }),
        }),
      },
      TestCase {
        input: "some avg10=0.00 avg60=0.01 avg300=0.02 total=42\n",
        expected: Ok(Pressure {
          some: PressureLine {
            avg10: Ratio::new::<percent>(0.00),
            avg60: Ratio::new::<percent>(0.01),
            avg300: Ratio::new::<percent>(0.02),
            total: Time::new::<microsecond>(42),
          },
          full: None,
        }),
      },
      TestCase {
        input: indoc! {"
          future avg10=99.00
          some total=7 future=99 avg300=3.00 avg10=1.00 avg60=2.00
        "},
        expected: Ok(Pressure {
          some: PressureLine {
            avg10: Ratio::new::<percent>(1.00),
            avg60: Ratio::new::<percent>(2.00),
            avg300: Ratio::new::<percent>(3.00),
            total: Time::new::<microsecond>(7),
          },
          full: None,
        }),
      },
      TestCase {
        input: "some avg10=1.00 avg60=2.00 total=4\n",
        expected: Err(
          "cgroup content \"some avg10=1.00 avg60=2.00 total=4\\n\" has \
           missing field \"avg300\"",
        ),
      },
      TestCase {
        input: "some avg10=nope avg60=0.00 avg300=0.00 total=0\n",
        expected: Err(
          "cgroup content \"some avg10=nope avg60=0.00 avg300=0.00 \
           total=0\\n\" has an invalid field \"avg10\" value \"nope\"",
        ),
      },
      TestCase {
        input: "some avg10=NaN avg60=0.00 avg300=0.00 total=0\n",
        expected: Err(
          "cgroup content \"some avg10=NaN avg60=0.00 avg300=0.00 \
           total=0\\n\" has an invalid field \"avg10\" value \"NaN\"",
        ),
      },
      TestCase {
        input: "some avg10=-0.01 avg60=0.00 avg300=0.00 total=0\n",
        expected: Err(
          "cgroup content \"some avg10=-0.01 avg60=0.00 avg300=0.00 \
           total=0\\n\" has an invalid field \"avg10\" value \"-0.01\"",
        ),
      },
      TestCase {
        input: "some avg10=100.01 avg60=0.00 avg300=0.00 total=0\n",
        expected: Err(
          "cgroup content \"some avg10=100.01 avg60=0.00 avg300=0.00 \
           total=0\\n\" has an invalid field \"avg10\" value \"100.01\"",
        ),
      },
    ];

    for case in cases {
      let actual = case.input.parse::<Pressure>();
      match case.expected {
        | Ok(expected) => {
          let actual = assert_ok!(actual, "input: {:?}", case.input);
          assert_eq!(actual, expected, "input: {:?}", case.input);
        },
        | Err(message) => {
          let actual = assert_err!(actual, "input: {:?}", case.input);
          assert_eq!(actual.to_string(), message, "input: {:?}", case.input);
        },
      }
    }
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn parses_live_root_cpu_pressure_when_available() {
    let path = "/sys/fs/cgroup/cpu.pressure";
    let contents = match std::fs::read_to_string(path) {
      | Ok(contents) => contents,
      | Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
      | Err(error) => panic!("failed to read {path}: {error}"),
    };

    assert_ok!(contents.parse::<Pressure>());
  }
}
