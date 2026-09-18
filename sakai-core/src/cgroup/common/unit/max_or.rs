/// A cgroup value that is either unlimited (`max`) or has a concrete value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaxOr<T> {
  /// The cgroup interface file contains `max`.
  Max,
  /// The cgroup interface file contains a concrete value.
  Value(T),
}
