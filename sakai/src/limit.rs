/// A cgroup value that preserves the kernel's `max` token or a concrete value.
///
/// For limits, `max` means no local limit. Protections and accounting
/// interfaces define its meaning separately; for example, device-memory
/// usage uses `max` for a kernel page-counter sentinel. Consult the interface
/// documentation before interpreting the token.
///
/// See [Linux cgroup limits and the `max` token](https://docs.kernel.org/admin-guide/cgroup-v2.html#limits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MaxOr<T> {
  /// The cgroup interface file contains `max`.
  ///
  /// See [Linux cgroup limits and the `max` token](https://docs.kernel.org/admin-guide/cgroup-v2.html#limits).
  Max,
  /// The cgroup interface file contains a concrete value.
  ///
  /// See [Linux cgroup limits and the `max` token](https://docs.kernel.org/admin-guide/cgroup-v2.html#limits).
  Value(T),
}

impl<T> MaxOr<T> {
  /// Transforms a concrete value and preserves the `max` token.
  pub fn map<U>(self, convert: impl FnOnce(T) -> U) -> MaxOr<U> {
    match self {
      | Self::Max => MaxOr::Max,
      | Self::Value(value) => MaxOr::Value(convert(value)),
    }
  }
}
