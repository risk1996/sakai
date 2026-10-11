use std::str::FromStr;

use crate::error::{ParseError, ParseValueError};

/// A volatile snapshot of `cpuset.cpus.partition` for one non-root cgroup.
///
/// Partition roots have exclusive CPUs; invalid roots retain configuration
/// but behave like members. Hotplug or ancestor changes can alter validity.
/// Reasons are diagnostic kernel text, retained verbatim rather than treated
/// as a fixed vocabulary. Older kernels report `root invalid` without a reason.
/// The hierarchy root is implicitly a partition root and lacks this file.
/// See [Linux cpuset interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html#cpuset-interface-files).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CpusetPartition {
  /// A non-root member of a partition.
  Member,
  /// A valid partition root with scheduler load balancing.
  Root,
  /// A valid partition root without scheduler load balancing.
  ///
  /// Since Linux 6.1; see the [Linux 6.1 source](https://github.com/torvalds/linux/blob/v6.1/kernel/cgroup/cpuset.c).
  Isolated,
  /// An invalid partition root, behaving like a member.
  RootInvalid {
    /// Kernel diagnostic text; `None` when an older kernel omits it.
    reason: Option<String>,
  },
  /// An invalid isolated partition root, behaving like a member.
  ///
  /// Since Linux 6.1; see the [Linux 6.1 source](https://github.com/torvalds/linux/blob/v6.1/kernel/cgroup/cpuset.c).
  IsolatedInvalid {
    /// Kernel diagnostic text; `None` when no reason is reported.
    reason: Option<String>,
  },
}

impl CpusetPartition {
  /// The cgroup v2 partition-state interface filename, absent at the root.
  pub const FILE_NAME: &'static str = "cpuset.cpus.partition";

  /// Inspects the stored base state: `member`, `root`, or `isolated`.
  ///
  /// Invalid roots retain their base state; inspect [`Self::is_invalid`] too.
  #[must_use]
  pub const fn state(&self) -> &'static str {
    match self {
      | Self::Member => "member",
      | Self::Root | Self::RootInvalid { .. } => "root",
      | Self::Isolated | Self::IsolatedInvalid { .. } => "isolated",
    }
  }

  /// Tests whether the stored snapshot describes an invalid partition root.
  ///
  /// Members are not invalid roots. This getter performs no kernel I/O.
  #[must_use]
  pub const fn is_invalid(&self) -> bool {
    match self {
      | Self::Member | Self::Root | Self::Isolated => false,
      | Self::RootInvalid { .. } | Self::IsolatedInvalid { .. } => true,
    }
  }

  /// Inspects the stored diagnostic reason, without parentheses or kernel I/O.
  ///
  /// `None` means no reason was reported; it does not imply a valid partition.
  #[must_use]
  pub fn reason(&self) -> Option<&str> {
    match self {
      | Self::Member | Self::Root | Self::Isolated => None,
      | Self::RootInvalid { reason } | Self::IsolatedInvalid { reason } => reason.as_deref(),
    }
  }
}

impl FromStr for CpusetPartition {
  type Err = ParseError<ParseValueError>;

  fn from_str(raw: &str) -> Result<Self, Self::Err> {
    let value = raw.trim();
    let invalid = || ParseError::invalid(raw, "partition", value, ParseValueError::OutOfRange);
    match value {
      | "" => Err(ParseError::missing(raw, "partition")),
      | "member" => Ok(Self::Member),
      | "root" => Ok(Self::Root),
      | "isolated" => Ok(Self::Isolated),
      | "root invalid" => Ok(Self::RootInvalid { reason: None }),
      | "isolated invalid" => Ok(Self::IsolatedInvalid { reason: None }),
      | _ => {
        let (state, reason) = value.split_once(" invalid (").ok_or_else(invalid)?;
        let reason = reason
          .strip_suffix(')')
          .filter(|reason| !reason.trim().is_empty() && !reason.contains(['\n', '\r']))
          .ok_or_else(invalid)?;
        match state {
          | "root" => Ok(Self::RootInvalid { reason: Some(reason.into()) }),
          | "isolated" => Ok(Self::IsolatedInvalid { reason: Some(reason.into()) }),
          | _ => Err(invalid()),
        }
      },
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::parse::tests::{
    Cases,
    Failure::{Invalid, Missing},
  };

  #[test]
  fn parses_current_and_older_partition_states() {
    Cases::<CpusetPartition>::check([
      ("member\n", Ok(CpusetPartition::Member)),
      (" root \n", Ok(CpusetPartition::Root)),
      ("isolated", Ok(CpusetPartition::Isolated)),
      (include_str!("../fixtures/p4_older/cpuset.cpus.partition"), Ok(CpusetPartition::RootInvalid { reason: None })),
      ("isolated invalid", Ok(CpusetPartition::IsolatedInvalid { reason: None })),
      (
        include_str!("../fixtures/p4/cpuset.cpus.partition"),
        Ok(CpusetPartition::RootInvalid { reason: Some("Parent is not a partition root".into()) }),
      ),
      (
        "isolated invalid (future reason (detail))",
        Ok(CpusetPartition::IsolatedInvalid { reason: Some("future reason (detail)".into()) }),
      ),
      ("", Err(Missing("partition"))),
      (" \n", Err(Missing("partition"))),
      ("future", Err(Invalid("partition", "future"))),
      ("member invalid", Err(Invalid("partition", "member invalid"))),
      ("root invalid ()", Err(Invalid("partition", "root invalid ()"))),
      ("root invalid ( )", Err(Invalid("partition", "root invalid ( )"))),
      ("root invalid (reason", Err(Invalid("partition", "root invalid (reason"))),
      ("root invalid (reason) extra", Err(Invalid("partition", "root invalid (reason) extra"))),
      ("root invalid reason", Err(Invalid("partition", "root invalid reason"))),
      ("member invalid (reason)", Err(Invalid("partition", "member invalid (reason)"))),
      ("root\nmember", Err(Invalid("partition", "root\nmember"))),
    ]);
  }
}
