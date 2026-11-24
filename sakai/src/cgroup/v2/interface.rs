//! Typed cgroup v2 file declarations and access capabilities.

use std::borrow::Cow;

use crate::error::{EncodeError, ParseError, ParseValueError};

pub(crate) mod sealed {
  pub trait File {}
}

/// A declared cgroup v2 file with an optional filename key.
pub trait CgroupFile: sealed::File {
  /// Exact filename, or its template when a key is required.
  ///
  /// Use [`Self::filename`] to resolve a template for a specific key.
  const FILE_NAME: &'static str;
  /// Filename selector, or `()` for a fixed filename.
  type Key: ?Sized;

  /// Resolves the filename for this key.
  fn filename(_key: &Self::Key) -> Cow<'static, str> { Cow::Borrowed(Self::FILE_NAME) }
}

/// A cgroup file with a decoder for its stored value.
///
/// ```compile_fail,E0277
/// use sakai::v2::{ReadableFile, cpu::CpuQuota};
/// fn requires_read<F: ReadableFile>() {}
/// requires_read::<CpuQuota>();
/// ```
pub trait ReadableFile: CgroupFile {
  /// Value obtained by reading the file.
  type Value;

  /// Decodes the contents of a complete read.
  ///
  /// # Errors
  /// Returns the parser error if the contents are invalid.
  fn decode(contents: &str) -> Result<Self::Value, ParseError<ParseValueError>>;
}

/// A cgroup file with an encoder for its write command.
///
/// ```compile_fail,E0277
/// use sakai::v2::{WritableFile, memory::MemoryCurrent};
/// fn requires_write<F: WritableFile>() {}
/// requires_write::<MemoryCurrent>();
/// ```
pub trait WritableFile: CgroupFile {
  /// Command accepted by the file, which may borrow its inputs.
  type Command<'a>;

  /// Encodes one complete write command.
  ///
  /// # Errors
  /// Returns an encoding error if the command cannot be represented.
  fn encode(command: Self::Command<'_>) -> Result<String, EncodeError>;
}

macro_rules! cgroup_file {
  (alias $(#[$doc:meta])* $file:ident, $name:expr => $value:ty $(, $decode:expr)?) => {
    $(#[$doc])*
    #[non_exhaustive]
    pub struct $file;
    $crate::v2::interface::cgroup_file!(file $file, $name);
    $crate::v2::interface::cgroup_file!(read $file => $value $(, $decode)?);
  };
  (file $file:ty, $name:expr $(, $key:ty => $resolve:expr)?) => {
    impl $crate::v2::interface::sealed::File for $file {}
    impl $crate::v2::CgroupFile for $file {
      const FILE_NAME: &'static str = $name;
      type Key = $crate::v2::interface::cgroup_file!(@key $($key)?);
      $(fn filename(key: &Self::Key) -> std::borrow::Cow<'static, str> { ($resolve)(key).into() })?
    }
  };
  (@key $key:ty) => { $key };
  (@key) => { () };
  (read $file:ty => $value:ty $(, $decode:expr)?) => {
    impl $crate::v2::ReadableFile for $file {
      type Value = $value;
      fn decode(contents: &str) -> Result<Self::Value, $crate::error::ParseError<$crate::error::ParseValueError>> {
        $crate::v2::interface::cgroup_file!(@decode contents $(, $decode)?)
      }
    }
  };
  (@decode $contents:ident) => { $contents.parse() };
  (@decode $contents:ident, $decode:expr) => { ($decode)($contents) };
  (write $file:ty, $borrow:lifetime => $command:ty, $encode:expr) => {
    impl $crate::v2::WritableFile for $file {
      type Command<$borrow> = $command;
      fn encode(command: Self::Command<'_>) -> Result<String, $crate::error::EncodeError> {
        ($encode)(command)
      }
    }
  };
  ($value:ty, $name:expr, Read) => {
    $crate::v2::interface::cgroup_file!(file $value, $name);
    $crate::v2::interface::cgroup_file!(read $value => $value);
  };
  ($value:ty, $name:expr, ReadWrite) => {
    $crate::v2::interface::cgroup_file!(file $value, $name);
    $crate::v2::interface::cgroup_file!(read $value => $value);
    $crate::v2::interface::cgroup_file!(write $value, 'a => &'a $value, <$value>::encode_cgroup);
  };
}
pub(crate) use cgroup_file;

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
