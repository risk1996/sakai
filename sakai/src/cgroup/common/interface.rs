//! Typed cgroup file declarations and access capabilities.

use std::borrow::Cow;

use crate::error::{EncodeError, ParseError, ParseValueError};

pub(crate) mod sealed {
  pub trait File {}
}

/// A declared cgroup file with an optional filename key.
///
/// A handle can only access file declarations for its own hierarchy version.
///
/// ```compile_fail,E0271
/// use sakai::{cgroup::{common::V1, ReadableFile}, v2::cpu::CpuMax};
/// fn requires_v1<F: ReadableFile<Version = V1>>() {}
/// requires_v1::<CpuMax>();
/// ```
pub trait CgroupFile: sealed::File {
  /// Exact filename, or its template when a key is required.
  ///
  /// Use [`Self::filename`] to resolve a template for a specific key.
  const FILE_NAME: &'static str;
  /// Hierarchy version in which this file is declared.
  type Version;
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
  (alias $(#[$doc:meta])* $file:ident, $name:expr => $value:ty $(, $decode:expr)? $(; $version:ty)?) => {
    $(#[$doc])*
    #[non_exhaustive]
    pub struct $file;
    $crate::cgroup::common::interface::cgroup_file!(file $file, $name $(; $version)?);
    $crate::cgroup::common::interface::cgroup_file!(read $file => $value $(, $decode)?);
  };
  (file $file:ty, $name:expr $(, $key:ty => $resolve:expr)? $(; $version:ty)?) => {
    impl $crate::cgroup::common::interface::sealed::File for $file {}
    impl $crate::cgroup::CgroupFile for $file {
      type Version = $crate::cgroup::common::interface::cgroup_file!(@version $($version)?);
      const FILE_NAME: &'static str = $name;
      type Key = $crate::cgroup::common::interface::cgroup_file!(@key $($key)?);
      $(fn filename(key: &Self::Key) -> std::borrow::Cow<'static, str> { ($resolve)(key).into() })?
    }
  };
  (@version $version:ty) => { $version };
  (@version) => { $crate::cgroup::common::V2 };
  (@key $key:ty) => { $key };
  (@key) => { () };
  (read $file:ty => $value:ty $(, $decode:expr)?) => {
    impl $crate::cgroup::ReadableFile for $file {
      type Value = $value;
      fn decode(contents: &str) -> Result<Self::Value, $crate::error::ParseError<$crate::error::ParseValueError>> {
        $crate::cgroup::common::interface::cgroup_file!(@decode contents $(, $decode)?)
      }
    }
  };
  (@decode $contents:ident) => { $contents.parse() };
  (@decode $contents:ident, $decode:expr) => { ($decode)($contents) };
  (write $file:ty, $borrow:lifetime => $command:ty, $encode:expr) => {
    impl $crate::cgroup::WritableFile for $file {
      type Command<$borrow> = $command;
      fn encode(command: Self::Command<'_>) -> Result<String, $crate::error::EncodeError> {
        ($encode)(command)
      }
    }
  };
  ($value:ty, $name:expr, Read $(; $version:ty)?) => {
    $crate::cgroup::common::interface::cgroup_file!(file $value, $name $(; $version)?);
    $crate::cgroup::common::interface::cgroup_file!(read $value => $value);
  };
  ($value:ty, $name:expr, ReadWrite $(; $version:ty)?) => {
    $crate::cgroup::common::interface::cgroup_file!(file $value, $name $(; $version)?);
    $crate::cgroup::common::interface::cgroup_file!(read $value => $value);
    $crate::cgroup::common::interface::cgroup_file!(write $value, 'a => &'a $value, <$value>::encode_cgroup);
  };
}
pub(crate) use cgroup_file;

pub(in crate::cgroup) fn check_token(value: &str, field: &'static str) -> Result<(), EncodeError> {
  if value.is_empty() || value.bytes().any(|byte| byte.is_ascii_whitespace() || byte == 0) {
    return Err(EncodeError { field });
  }
  Ok(())
}

pub(in crate::cgroup) fn encode_keyed<Unit>(
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

macro_rules! cgroup_scalar_snapshot {
  ($(#[$doc:meta])* $name:ident, $value:ty, $codec:ty, $field:literal) => {
    $(#[$doc])*
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub struct $name {
      /// Value reported by the kernel.
      pub value: $value,
    }

    $crate::parse::cgroup_value!(scalar $name, value: $codec, $field);
  };
}

macro_rules! cgroup_scalars {
  ($version:ty; $($(#[$doc:meta])* $name:ident, $file:literal, $value:ty, $codec:ty, $access:ident;)+) => {
    $(
      $crate::cgroup::common::interface::cgroup_scalar_snapshot!($(#[$doc])* $name, $value, $codec, $file);
      $crate::cgroup::common::interface::cgroup_file!($name, $file, Read; $version);
      $crate::cgroup::common::interface::cgroup_scalars!(@write $name, $value, $access);
    )+
  };
  (@write $name:ident, $value:ty, Read) => {};
  (@write $name:ident, $value:ty, ReadWrite) => {
    $crate::cgroup::common::interface::cgroup_file!(write $name, 'a => $value, |value| $name { value }.encode_cgroup());
  };
}

pub(crate) use cgroup_scalar_snapshot;
pub(crate) use cgroup_scalars;

macro_rules! cgroup_hugetlb_file {
  ($file:ty, $suffix:literal $(; $version:ty)?) => {
    $crate::cgroup::common::interface::cgroup_file!(file $file, concat!("hugetlb.<hugepagesize>.", $suffix),
      $crate::cgroup::common::PageSize => |size: &$crate::cgroup::common::PageSize| size.file_name($suffix)
      $(; $version)?);
  };
}
pub(crate) use cgroup_hugetlb_file;
