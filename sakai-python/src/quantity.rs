//! Native Python quantities backed by Sakai's Rust units.

use pyo3::{prelude::*, types::PyModule};
use uom::si::{
  frequency::hertz,
  information::byte,
  information_rate::byte_per_second,
  ratio::{percent, ratio},
  time::nanosecond,
};

/// Retains a Rust quantity until it is exposed as its concrete Python class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Quantity<Q>(pub(crate) Q);

impl<Q> From<Q> for Quantity<Q> {
  fn from(value: Q) -> Self { Self(value) }
}

impl<Q: Copy> From<&Q> for Quantity<Q> {
  fn from(value: &Q) -> Self { Self(*value) }
}

macro_rules! quantities {
  ($(
    $(#[$class_doc:meta])*
    $name:ident($value:ident) in $unit:literal {
      $(#[$base_doc:meta])*
      $base:ident -> $scalar:ty = $extract:expr;
      $($(#[$doc:meta])* $getter:ident -> $result:ty = $convert:expr;)*
    }
  )*) => {
    #[cfg(all(feature = "stubs", target_os = "linux"))]
    pub(crate) const QUANTITY_CLASS_NAMES: &[&str] = &[$(stringify!($name)),*];

    pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
      $(module.add_class::<$name>()?;)*
      Ok(())
    }

    $(
      $(#[$class_doc])*
      #[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pyclass)]
      #[pyclass(frozen, module = "sakai._sakai")]
      pub(crate) struct $name(sakai::$name);

      #[cfg_attr(feature = "stubs", pyo3_stub_gen::derive::gen_stub_pymethods)]
      #[pymethods]
      impl $name {
        $(#[$base_doc])*
        #[getter]
        fn $base(&$value) -> $scalar { $extract }

        $(
          $(#[$doc])*
          #[getter]
          fn $getter(&$value) -> $result { $convert }
        )*

        fn __repr__(&self) -> String {
          format!("{}({} {})", stringify!($name), self.$base(), $unit)
        }
      }

      impl<'py> IntoPyObject<'py> for Quantity<sakai::$name> {
        type Error = PyErr;
        type Output = Bound<'py, $name>;
        type Target = $name;

        fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> {
          Bound::new(py, $name(self.0))
        }
      }

      impl<'py> IntoPyObject<'py> for &Quantity<sakai::$name> {
        type Error = PyErr;
        type Output = Bound<'py, $name>;
        type Target = $name;

        fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> { (*self).into_pyobject(py) }
      }

      #[cfg(feature = "stubs")]
      impl pyo3_stub_gen::PyStubType for Quantity<sakai::$name> {
        fn type_output() -> pyo3_stub_gen::TypeInfo {
          <$name as pyo3_stub_gen::PyStubType>::type_output()
        }
      }

      #[cfg(all(test, target_os = "linux"))]
      impl<'a, 'py> FromPyObject<'a, 'py> for Quantity<sakai::$name> {
        type Error = PyErr;

        fn extract(object: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
          Ok(Self(object.extract::<PyRef<'_, $name>>()?.0))
        }
      }
    )*
  };
}

#[expect(
  clippy::cast_precision_loss,
  reason = "fractional accessors round large values; canonical accessors are exact"
)]
fn fractional(value: u64, units: f64) -> f64 { value as f64 / units }

quantities! {
  /// A duration stored in integer nanoseconds.
  Time(self) in "ns" {
    /// Exact duration in nanoseconds.
    nanoseconds -> u64 = self.0.get::<nanosecond>();
    /// Duration in microseconds, including fractions.
    microseconds -> f64 = fractional(self.nanoseconds(), 1_000.0);
    /// Duration in milliseconds, including fractions.
    milliseconds -> f64 = fractional(self.nanoseconds(), 1_000_000.0);
    /// Duration in seconds, including fractions.
    seconds -> f64 = fractional(self.nanoseconds(), 1_000_000_000.0);
  }
  /// An amount of information stored in integer bytes.
  Bytes(self) in "B" {
    /// Exact amount in bytes.
    bytes -> u64 = self.0.get::<byte>();
    /// Exact amount in bits.
    bits -> u128 = u128::from(self.bytes()) * 8;
    /// Amount in kibibytes, including fractions.
    kibibytes -> f64 = fractional(self.bytes(), 1024.0);
    /// Amount in mebibytes, including fractions.
    mebibytes -> f64 = fractional(self.bytes(), 1024.0_f64.powi(2));
    /// Amount in gibibytes, including fractions.
    gibibytes -> f64 = fractional(self.bytes(), 1024.0_f64.powi(3));
  }
  /// A number of tasks, events, or other counted objects.
  Count(self) in "count" {
    /// Exact number of counted objects.
    count -> u64 = self.0.value;
  }
  /// A number of memory pages, independent of page size.
  Pages(self) in "pages" {
    /// Exact number of pages.
    pages -> u64 = self.0.value;
  }
  /// An information rate stored in integer bytes per second.
  BytesPerSecond(self) in "B/s" {
    /// Exact rate in bytes per second.
    bytes_per_second -> u64 = self.0.get::<byte_per_second>();
    /// Rate in kibibytes per second, including fractions.
    kibibytes_per_second -> f64 = fractional(self.bytes_per_second(), 1024.0);
    /// Rate in mebibytes per second, including fractions.
    mebibytes_per_second -> f64 = fractional(self.bytes_per_second(), 1024.0_f64.powi(2));
    /// Rate in gibibytes per second, including fractions.
    gibibytes_per_second -> f64 = fractional(self.bytes_per_second(), 1024.0_f64.powi(3));
  }
  /// An I/O operation rate stored in integer operations per second.
  OperationsPerSecond(self) in "ops/s" {
    /// Exact rate in operations per second.
    operations_per_second -> u64 = self.0.get::<hertz>();
  }
  /// A dimensionless ratio, such as CPU utilization or a pressure average.
  Ratio(self) in "ratio" {
    /// The dimensionless value as a fraction, such as 0.5 for 50%.
    fraction -> f64 = self.0.get::<ratio>();
    /// The value as a percentage, such as 50.0 for 50%.
    percent -> f64 = self.0.get::<percent>();
  }
}

#[cfg(all(test, target_os = "linux"))]
mod fixtures {
  use super::*;

  macro_rules! from_magnitude {
    ($type:ident, $scalar:ty, $value:ident => $construct:expr) => {
      impl From<$scalar> for Quantity<sakai::$type> {
        fn from($value: $scalar) -> Self { Self($construct) }
      }
    };
  }

  from_magnitude!(Time, u64, value => sakai::Time::new::<nanosecond>(value));
  from_magnitude!(Bytes, u64, value => sakai::Bytes::new::<byte>(value));
  from_magnitude!(Count, u64, value => sakai::Count { value, ..Default::default() });
  from_magnitude!(Pages, u64, value => sakai::Pages { value, ..Default::default() });
  from_magnitude!(BytesPerSecond, u64, value => sakai::BytesPerSecond::new::<byte_per_second>(value));
  from_magnitude!(OperationsPerSecond, u64, value => sakai::OperationsPerSecond::new::<hertz>(value));
  from_magnitude!(Ratio, f64, value => sakai::Ratio::new::<ratio>(value));
}

#[cfg(test)]
mod tests {
  use assertables::{assert_in_delta, assert_ok};

  use super::*;

  #[test]
  fn preserves_integer_boundaries() {
    for value in [0, 1, (1_u64 << 53) + 1, u64::MAX] {
      assert_eq!(Time(sakai::Time::new::<nanosecond>(value)).nanoseconds(), value);
      let information = Bytes(sakai::Bytes::new::<byte>(value));
      assert_eq!(information.bytes(), value);
      assert_eq!(information.bits(), u128::from(value) * 8);
      assert_eq!(Count(sakai::Count { value, ..Default::default() }).count(), value);
      assert_eq!(Pages(sakai::Pages { value, ..Default::default() }).pages(), value);
      assert_eq!(BytesPerSecond(sakai::BytesPerSecond::new::<byte_per_second>(value)).bytes_per_second(), value);
      assert_eq!(OperationsPerSecond(sakai::OperationsPerSecond::new::<hertz>(value)).operations_per_second(), value);
    }
  }

  #[test]
  fn exposes_fractional_conversions_to_python() {
    Python::initialize();
    Python::attach(|py| {
      let time = assert_ok!(Quantity(sakai::Time::new::<nanosecond>(1500)).into_pyobject(py));
      assert_in_delta!(assert_ok!(assert_ok!(time.getattr("microseconds")).extract::<f64>()), 1.5, f64::EPSILON);
      let bytes = assert_ok!(Quantity(sakai::Bytes::new::<byte>(1536)).into_pyobject(py));
      assert_in_delta!(assert_ok!(assert_ok!(bytes.getattr("kibibytes")).extract::<f64>()), 1.5, f64::EPSILON);
      let utilization = assert_ok!(Quantity(sakai::Ratio::new::<ratio>(0.125)).into_pyobject(py));
      assert_in_delta!(assert_ok!(assert_ok!(utilization.getattr("percent")).extract::<f64>()), 12.5, f64::EPSILON);
      let module = assert_ok!(PyModule::new(py, "sakai._sakai"));
      assert_ok!(register(&module));
      assert!(time.get_type().is(&assert_ok!(module.getattr("Time"))));
    });
  }
}
