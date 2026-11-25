pub(crate) use crate::cgroup::interface::cgroup_file;
use crate::error::EncodeError;

pub(super) fn check_token(value: &str, field: &'static str) -> Result<(), crate::error::EncodeError> {
  if value.is_empty() || value.bytes().any(|byte| byte.is_ascii_whitespace() || byte == 0) {
    return Err(crate::error::EncodeError { field });
  }
  Ok(())
}

pub(super) fn encode_keyed<Unit>(
  key: &str,
  key_field: &'static str,
  value: &impl crate::parse::EncodeCgroup<Unit>,
  value_field: &'static str,
) -> Result<String, EncodeError> {
  check_token(key, key_field)?;
  let mut command = format!("{key} ");
  value.encode_field(&mut command, value_field)?;
  Ok(command)
}
