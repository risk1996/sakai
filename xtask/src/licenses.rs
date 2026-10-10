use std::{path::Path, process::Command};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Dependency {
  name: String,
  version: String,
  license: Option<String>,
}

pub struct Licenses;

impl Licenses {
  // cargo-license normalizes SPDX expressions. Exact matches intentionally
  // require review for new combinations, preserving AND/OR/WITH semantics.
  // For expressions offering copyleft alternatives, we choose MIT/Apache-2.0.
  const REVIEWED_EXPRESSIONS: &[&str] = &[
    "(Apache-2.0 OR MIT) AND Unicode-3.0",
    "Apache-2.0",
    "Apache-2.0 AND ISC",
    "Apache-2.0 OR Apache-2.0 WITH LLVM-exception OR MIT",
    "Apache-2.0 OR BSD-3-Clause OR GPL-2.0 OR GPL-3.0 OR MIT",
    "Apache-2.0 OR BSL-1.0",
    "Apache-2.0 OR ISC OR MIT",
    "Apache-2.0 OR LGPL-2.1-or-later OR MIT",
    "Apache-2.0 OR MIT",
    "Apache-2.0 OR MIT OR Zlib",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-3-Clause",
    "CDLA-Permissive-2.0",
    "ISC",
    "MIT",
    "MIT OR Unlicense",
    "Unicode-3.0",
  ];

  pub fn check() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().context("xtask must be inside the workspace")?;
    // Verify the same metadata query with --locked before cargo-license, which
    // has no --locked option. Run both offline against the fetched lockfile.
    let metadata = Command::new("cargo")
      .args(["metadata", "--locked", "--offline", "--all-features", "--format-version", "1"])
      .current_dir(workspace)
      .output()
      .context("verify locked dependency metadata")?;
    ensure!(
      metadata.status.success(),
      "locked dependency metadata failed (run devenv tasks run check:licenses to fetch dependencies): {}",
      String::from_utf8_lossy(&metadata.stderr)
    );
    let output = Command::new("cargo")
      .args(["license", "--all-features", "--json"])
      .current_dir(workspace)
      .env("CARGO_NET_OFFLINE", "true")
      .output()
      .context("run cargo-license (available through devenv)")?;
    ensure!(output.status.success(), "cargo-license failed: {}", String::from_utf8_lossy(&output.stderr));
    let count = Self::validate(&output.stdout)?;
    println!("License policy passed for {count} workspace packages and dependencies.");
    Ok(())
  }

  fn validate(report: &[u8]) -> Result<usize> {
    let dependencies: Vec<Dependency> = serde_json::from_slice(report).context("parse cargo-license JSON report")?;
    ensure!(!dependencies.is_empty(), "cargo-license returned an empty report");
    let violations = dependencies
      .iter()
      .filter(|dependency| match &dependency.license {
        | Some(expression) => !Self::REVIEWED_EXPRESSIONS.contains(&expression.as_str()),
        | None => true,
      })
      .map(|dependency| {
        format!(
          "{} {}: {}",
          dependency.name,
          dependency.version,
          dependency.license.as_deref().unwrap_or("missing license metadata")
        )
      })
      .collect::<Vec<_>>();
    match violations.is_empty() {
      | true => Ok(dependencies.len()),
      | false => bail!("Unreviewed dependency licenses:\n{}", violations.join("\n")),
    }
  }
}

#[cfg(test)]
mod tests {
  use anyhow::{Context, Result, ensure};
  use serde_json::json;

  use super::Licenses;

  #[test]
  fn reviewed_expressions_pass() -> Result<()> {
    for expression in Licenses::REVIEWED_EXPRESSIONS {
      let report = json!([{ "name": "example", "version": "1.0.0", "license": expression }]);
      ensure!(Licenses::validate(&serde_json::to_vec(&report)?)? == 1, "reviewed expression must pass: {expression}");
    }
    Ok(())
  }

  #[test]
  fn unreviewed_expressions_fail() -> Result<()> {
    for expression in [
      None,
      Some(""),
      Some("GPL-3.0-only"),
      Some("LGPL-2.1-or-later"),
      Some("MIT AND GPL-3.0-only"),
      Some("MIT WITH LLVM-exception"),
      Some("LicenseRef-Custom"),
      Some("MIT OR"),
      Some("MIT OR LicenseRef-Custom"),
    ] {
      // One valid dependency must not hide a violating dependency.
      let report = json!([
        { "name": "allowed", "version": "1.0.0", "license": "MIT" },
        { "name": "violation", "version": "2.0.0", "license": expression },
      ]);
      let error = Licenses::validate(&serde_json::to_vec(&report)?).err().context("license violation must fail")?;
      ensure!(error.to_string().contains("violation 2.0.0"), "diagnostic must identify the violating dependency");
    }
    Ok(())
  }

  #[test]
  fn invalid_reports_fail() -> Result<()> {
    for report in [b"[]".as_slice(), b"null", b"{}", b"not JSON", b"[{}]"] {
      Licenses::validate(report).err().context("invalid report must fail")?;
    }
    Ok(())
  }
}
