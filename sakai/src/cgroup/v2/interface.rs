//! Typed cgroup v2 file declarations and access capabilities.

use std::{borrow::Cow, error::Error as StdError};

use crate::error::EncodeError;

pub(crate) mod sealed {
  pub trait Mode {}
  pub trait File {}
}

/// A sealed access mode for a cgroup file.
pub trait AccessMode: sealed::Mode {}

/// Access modes supported by Sakai.
pub mod mode {
  use super::{AccessMode, sealed};

  /// An interface exposed for reading.
  #[non_exhaustive]
  pub struct Read;
  /// An interface exposed for writing.
  #[non_exhaustive]
  pub struct Write;
  /// An interface exposed for reading and writing.
  #[non_exhaustive]
  pub struct ReadWrite;

  impl sealed::Mode for Read {}
  impl sealed::Mode for Write {}
  impl sealed::Mode for ReadWrite {}
  impl AccessMode for Read {}
  impl AccessMode for Write {}
  impl AccessMode for ReadWrite {}
}

/// A mode that supports reading.
pub trait ReadMode: AccessMode {}
impl ReadMode for mode::Read {}
impl ReadMode for mode::ReadWrite {}

/// A mode that supports writing.
pub trait WriteMode: AccessMode {}
impl WriteMode for mode::Write {}
impl WriteMode for mode::ReadWrite {}

/// A declared cgroup v2 file, with its access mode and optional filename key.
///
/// Modes describe the operations offered by Sakai. Kernel availability and
/// permission checks still apply.
pub trait CgroupFile: sealed::File {
  /// Exact filename, or its template when a key is required.
  const FILE_NAME: &'static str;
  /// Access supported by this declaration.
  type Mode: AccessMode;
  /// Filename selector, or `()` for a fixed filename.
  type Key: ?Sized;

  /// Resolves the filename for this key.
  fn filename(_key: &Self::Key) -> Cow<'static, str> { Cow::Borrowed(Self::FILE_NAME) }
}

/// Decodes a file's stored value.
pub trait FileValue: CgroupFile {
  /// Value obtained by reading the file.
  type Value;

  /// Decodes the contents of a complete read.
  ///
  /// # Errors
  /// Returns the parser error if the contents are invalid.
  fn decode(contents: &str) -> Result<Self::Value, Box<dyn StdError + Send + Sync>>;
}

/// Encodes one command for a file.
pub trait FileCommand: CgroupFile {
  /// Command accepted by the file, which may borrow its inputs.
  type Command<'a>;

  /// Encodes one complete write command.
  ///
  /// # Errors
  /// Returns an encoding error if the command cannot be represented.
  fn encode(command: Self::Command<'_>) -> Result<String, EncodeError>;
}

/// A file whose mode and codec support reading.
pub trait ReadableFile: FileValue<Mode: ReadMode> {}
impl<F: FileValue<Mode: ReadMode>> ReadableFile for F {}

/// A file whose mode and codec support writing.
pub trait WritableFile: FileCommand<Mode: WriteMode> {}
impl<F: FileCommand<Mode: WriteMode>> WritableFile for F {}

macro_rules! cgroup_file {
  (alias $(#[$doc:meta])* $file:ident, $name:expr, $mode:ident, $value:ty $(, $decode:expr)?) => {
    $(#[$doc])*
    #[non_exhaustive]
    pub struct $file;
    $crate::v2::interface::cgroup_file!(file $file, $name, $mode);
    $crate::v2::interface::cgroup_file!(read $file => $value $(, $decode)?);
  };
  (file $file:ty, $name:expr, $mode:ident $(, $key:ty => $resolve:expr)?) => {
    impl $crate::v2::interface::sealed::File for $file {}
    impl $crate::v2::CgroupFile for $file {
      const FILE_NAME: &'static str = $name;
      type Mode = $crate::v2::mode::$mode;
      type Key = $crate::v2::interface::cgroup_file!(@key $($key)?);
      $(fn filename(key: &Self::Key) -> std::borrow::Cow<'static, str> { ($resolve)(key).into() })?
    }
  };
  (@key $key:ty) => { $key };
  (@key) => { () };
  (read $file:ty => $value:ty $(, $decode:expr)?) => {
    impl $crate::v2::FileValue for $file {
      type Value = $value;
      fn decode(contents: &str) -> Result<Self::Value, Box<dyn std::error::Error + Send + Sync>> {
        $crate::v2::interface::cgroup_file!(@decode contents $(, $decode)?)
      }
    }
  };
  (@decode $contents:ident) => { Ok($contents.parse()?) };
  (@decode $contents:ident, $decode:expr) => { ($decode)($contents) };
  (write $file:ty, $borrow:lifetime => $command:ty, $encode:expr) => {
    impl $crate::v2::FileCommand for $file {
      type Command<$borrow> = $command;
      fn encode(command: Self::Command<'_>) -> Result<String, $crate::error::EncodeError> {
        ($encode)(command)
      }
    }
  };
  ($value:ty, $name:expr, Read) => {
    $crate::v2::interface::cgroup_file!(file $value, $name, Read);
    $crate::v2::interface::cgroup_file!(read $value => $value);
  };
  ($value:ty, $name:expr, ReadWrite) => {
    $crate::v2::interface::cgroup_file!(file $value, $name, ReadWrite);
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
