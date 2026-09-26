mod vmtest;

use anyhow::Result;
use clap::{Parser, Subcommand};
use vmtest::Vmtest;

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
}

#[tokio::main]
async fn main() -> Result<()> {
  match Xtask::parse().command {
    | Command::Vmtest(command) => command.run().await,
    | Command::BuildLiveTest => Vmtest::build_live_test(),
  }
}
