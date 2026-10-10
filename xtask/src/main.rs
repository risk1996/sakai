use anyhow::Result;
use clap::{Parser, Subcommand};
use vmtest::{Profile, Vmtest};

mod vmtest;

#[derive(Debug, Parser)]
struct Xtask {
  #[command(subcommand)]
  command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
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
async fn main() -> Result<()> {
  match Xtask::parse().command {
    | Command::Vmtest(command) => command.run().await,
    | Command::BuildLiveTest => Vmtest::build_live_test(),
    | Command::KernelMatrix { profile } => Vmtest::kernel_matrix(profile),
  }
}
