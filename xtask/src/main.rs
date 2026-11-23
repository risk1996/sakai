use anyhow::Result;
use clap::{Parser, Subcommand};
use installation_size::InstallationSize;
use release::Release;
use vmtest::{Profile, Vmtest};

mod http;
mod installation_size;
mod release;
mod vmtest;

#[derive(Debug, Parser)]
struct Xtask {
  #[command(subcommand)]
  command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
  /// Prepare and verify stable release candidates without reading credentials.
  Release(Release),
  /// Measure the packages a clean downstream project downloads for Sakai.
  InstallationSize(InstallationSize),
  /// Run live cgroup tests in Linux microVMs.
  Vmtest(Vmtest),
  /// Build and stage the linux_live test executable for CI.
  BuildLiveTest,
  /// Print the kernel names as JSON for CI's job matrix.
  KernelMatrix {
    #[arg(long, value_enum, default_value_t = Profile::Smoke)]
    profile: Profile,
  },
}

#[tokio::main]
/// Dispatch explicitly selected developer tasks; shell entry does not initiate a release.
async fn main() -> Result<()> {
  match Xtask::parse().command {
    | Command::Release(command) => command.run().await,
    | Command::InstallationSize(command) => command.run(),
    | Command::Vmtest(command) => command.run(),
    | Command::BuildLiveTest => Vmtest::build_live_test(),
    | Command::KernelMatrix { profile } => Vmtest::kernel_matrix(profile),
  }
}
