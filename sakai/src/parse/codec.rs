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

/// Generates text decoding and encoding from the same positional fields.
macro_rules! cgroup_value {
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
