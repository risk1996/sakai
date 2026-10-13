use assertables::{assert_err, assert_ok};
use pyo3::{
  exceptions::{PyAttributeError, PyTypeError, PyValueError},
  prelude::*,
  types::PyInt,
};

pub(super) fn check_limit(limit: &Bound<'_, PyAny>, expected: Option<u64>) {
  assert_eq!(assert_ok!(assert_ok!(limit.getattr("is_max")).extract::<bool>()), expected.is_none());
  match expected {
    | Some(expected) => {
      let value = assert_ok!(limit.getattr("value"));
      assert!(value.is_instance_of::<PyInt>());
      assert_eq!(assert_ok!(value.extract::<u64>()), expected);
    },
    | None => assert!(assert_err!(limit.getattr("value")).is_instance_of::<PyValueError>(limit.py())),
  }
}

pub(super) fn check_frozen(object: &Bound<'_, PyAny>, names: &[&str]) {
  for name in names {
    assert!(assert_err!(object.setattr(*name, 0)).is_instance_of::<PyAttributeError>(object.py()));
  }
}

pub(super) fn readonly<'py, K: IntoPyObject<'py>>(object: &Bound<'py, PyAny>, name: &str, key: K) -> Bound<'py, PyAny> {
  let mapping = assert_ok!(object.getattr(name));
  assert!(assert_err!(mapping.set_item(key, 0)).is_instance_of::<PyTypeError>(object.py()));
  check_frozen(object, &[name]);
  mapping
}
