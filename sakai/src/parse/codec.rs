use super::ParseCgroup;
use crate::error::EncodeError;

/// Encodes a field with the same marker used to parse it.
pub(crate) trait EncodeCgroup<Unit>: ParseCgroup<Unit> {
  /// Appends one nonempty token if the value is exactly representable.
  fn encode_token(&self, output: &mut String) -> Option<()>;

  fn encode_field(&self, output: &mut String, field: &'static str) -> Result<(), EncodeError> {
    self.encode_token(output).ok_or(EncodeError { field })
  }
}

pub(crate) fn encode_pair<Unit, T: EncodeCgroup<Unit>>(
  output: &mut String,
  prefix: &str,
  delimiter: char,
  key: &'static str,
  value: &T,
) -> Result<(), EncodeError> {
  if !output.is_empty() {
    output.push_str(prefix);
  }
  output.push_str(key);
  output.push(delimiter);
  value.encode_field(output, key)
}

/// Generates cgroup text codecs from declared fields and token encodings.
macro_rules! cgroup_value {
  (display $value:ident) => {
    impl $value {
      /// Encodes the value using its canonical cgroup spelling.
      ///
      /// # Errors
      ///
      /// All values of this type are representable.
      pub fn encode_cgroup(&self) -> Result<String, $crate::error::EncodeError> {
        Ok(self.to_string())
      }
    }
  };
  (scalar $value:ident, $member:ident : $unit:ty, $field:literal) => {
    impl ::std::str::FromStr for $value {
      type Err = $crate::error::ParseError<$crate::error::ParseValueError>;

      fn from_str(contents: &str) -> Result<Self, Self::Err> {
        Ok(Self { $member: $crate::parse::Parser::single::<$unit, _>(contents, $field)? })
      }
    }

    impl $value {
      /// Encodes the stored scalar using the interface's units.
      ///
      /// # Errors
      ///
      /// Returns an error if the value cannot be represented exactly.
      pub fn encode_cgroup(&self) -> Result<String, $crate::error::EncodeError> {
        let mut output = String::new();
        $crate::parse::EncodeCgroup::<$unit>::encode_field(&self.$member, &mut output, $field)?;
        Ok(output)
      }
    }
  };
  (map $value:ident, $field:ident : $unit:ty, $label:literal) => {
    impl ::std::str::FromStr for $value {
      type Err = $crate::error::ParseError<$crate::error::ParseValueError>;

      fn from_str(contents: &str) -> Result<Self, Self::Err> {
        let fields = $crate::parse::KeyedFields::parse(contents, |_| $label)?;
        Ok(Self { $field: fields.parse_values::<$unit, _>($label)? })
      }
    }

    impl $value {
      /// Encodes the stored snapshot as canonical cgroup text.
      ///
      /// # Errors
      ///
      /// Returns an error if a value cannot be represented exactly.
      pub fn encode_cgroup(&self) -> Result<String, $crate::error::EncodeError> {
        let mut output = String::new();
        for (key, value) in &self.$field {
          if !output.is_empty() {
            output.push('\n');
          }
          output.push_str(key);
          output.push(' ');
          $crate::parse::EncodeCgroup::<$unit>::encode_field(value, &mut output, $label)?;
        }
        Ok(output)
      }
    }
  };
  (device_rows $value:ident, $field:ident : $row:ty, $parse:path) => {
    impl ::std::str::FromStr for $value {
      type Err = $crate::error::ParseError<$crate::error::ParseValueError>;

      fn from_str(contents: &str) -> Result<Self, Self::Err> {
        Ok(Self { $field: ($parse)(contents, |fields| <$row>::from_fields(&fields))? })
      }
    }

    impl $value {
      /// Encodes the stored device rows as canonical cgroup text.
      ///
      /// # Errors
      ///
      /// Returns an error if a field cannot be represented exactly.
      pub fn encode_cgroup(&self) -> Result<String, $crate::error::EncodeError> {
        let mut output = String::new();
        for (device, row) in &self.$field {
          if !output.is_empty() {
            output.push('\n');
          }
          output.push_str(&device.to_string());
          let start = output.len();
          row.encode_fields(&mut output, " ", '=')?;
          if output.len() == start {
            return Err($crate::error::EncodeError { field: "value" });
          }
        }
        Ok(output)
      }
    }
  };
  (token $value:ident : $unit:ty, $field:expr) => {
    impl ::std::str::FromStr for $value {
      type Err = $crate::error::ParseError<$crate::error::ParseValueError>;

      fn from_str(contents: &str) -> Result<Self, Self::Err> {
        $crate::parse::Parser::single::<$unit, Self>(contents, $field)
      }
    }

    impl $value {
      /// Encodes this value as a canonical cgroup token.
      ///
      /// # Errors
      ///
      /// Returns an error if the value cannot be represented exactly.
      pub fn encode_cgroup(&self) -> Result<String, $crate::error::EncodeError> {
        let mut output = String::new();
        $crate::parse::EncodeCgroup::<$unit>::encode_field(self, &mut output, $field)?;
        Ok(output)
      }
    }
  };
  (keyed $value:ident, $label:expr, $($fields:tt)+) => {
    $crate::parse::cgroup_value!(fields $value, $($fields)+);

    impl ::std::str::FromStr for $value {
      type Err = $crate::error::ParseError<$crate::error::ParseValueError>;

      fn from_str(contents: &str) -> Result<Self, Self::Err> {
        Self::from_fields(&$crate::parse::KeyedFields::parse(contents, $label)?)
      }
    }

    impl $value {
      /// Encodes the stored snapshot as canonical cgroup text.
      ///
      /// # Errors
      ///
      /// Returns an error if a field cannot be represented exactly.
      pub fn encode_cgroup(&self) -> Result<String, $crate::error::EncodeError> {
        let mut output = String::new();
        self.encode_fields(&mut output, "\n", ' ')?;
        Ok(output)
      }
    }
  };
  (fields $value:ident, { $($field:ident : $kind:ident $unit:ty => $key:expr),+ $(,)? }) => {
    impl $value {
      fn from_fields(fields: &$crate::parse::KeyedFields<'_>) -> Result<Self, $crate::error::ParseError<$crate::error::ParseValueError>> {
        Ok(Self { $($field: $crate::parse::cgroup_value!(@parse $kind, $unit, fields, $key)?,)+ })
      }

      fn encode_fields(&self, output: &mut String, prefix: &str, delimiter: char) -> Result<(), $crate::error::EncodeError> {
        $(
          $crate::parse::cgroup_value!(@field $kind, $unit, self.$field, $key, output, prefix, delimiter);
        )+
        Ok(())
      }
    }
  };
  (@parse group, $unit:ty, $fields:expr, $keys:expr) => {
    if $fields.contains_any($keys) {
      <$unit>::from_fields($fields).map(Some)
    } else {
      Ok(None)
    }
  };
  (@parse $kind:ident, $unit:ty, $fields:expr, $key:expr) => {
    $fields.$kind::<$unit, _>($key)
  };
  (@field group, $unit:ty, $value:expr, $keys:expr, $out:expr, $prefix:expr, $delimiter:expr) => {
    if let Some(group) = &$value {
      group.encode_fields($out, $prefix, $delimiter)?;
    }
  };
  (@field required, $unit:ty, $value:expr, $key:expr, $out:expr, $prefix:expr, $delimiter:expr) => {
    $crate::parse::encode_pair::<$unit, _>($out, $prefix, $delimiter, ::core::convert::Into::<&'static str>::into($key), &$value)?;
  };
  (@field optional, $unit:ty, $value:expr, $key:expr, $out:expr, $prefix:expr, $delimiter:expr) => {
    if let Some(value) = &$value {
      $crate::parse::encode_pair::<$unit, _>($out, $prefix, $delimiter, ::core::convert::Into::<&'static str>::into($key), value)?;
    }
  };
  (positional $value:ident { $($field:ident : $unit:ty),+ $(,)? }) => {
    impl ::std::str::FromStr for $value {
      type Err = $crate::error::ParseError<$crate::error::ParseValueError>;

      fn from_str(contents: &str) -> Result<Self, Self::Err> {
        $crate::parse::Parser::parse(contents, |parser| {
          Ok(Self { $($field: parser.next_field::<$unit, _>(stringify!($field))?,)+ })
        })
      }
    }

    impl $value {
      /// Encodes this value as canonical cgroup text.
      ///
      /// Fields are separated by a single space, with no trailing newline.
      ///
      /// # Errors
      ///
      /// Returns an error if a field cannot be represented exactly in the
      /// interface's units.
      pub fn encode_cgroup(&self) -> Result<String, $crate::error::EncodeError> {
        let mut output = String::new();
        $(
          if !output.is_empty() {
            output.push(' ');
          }
          $crate::parse::EncodeCgroup::<$unit>::encode_field(&self.$field, &mut output, stringify!($field))?;
        )+
        Ok(output)
      }
    }
  };
}

pub(crate) use cgroup_value;
