/// A cgroup value that is either unlimited (`max`) or has a concrete value.
///
/// See [Linux cgroup limits and the `max` token](https://docs.kernel.org/admin-guide/cgroup-v2.html#limits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
