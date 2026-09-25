mod build;
mod diagnostics;
mod kernel;

use std::{
  env, fs,
  io::{self, Write},
  path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use boxlite::{
  AttachOptions, BoxOptions, BoxliteOptions, RootfsSpec,
  experimental::{
    ExperimentalFeature, RuntimeBuilder,
    custom_kernel::{self, KernelOptions},
  },
  runtime::{
    advanced_options::{AdvancedBoxOptions, ContainerCapabilities},
    options::VolumeSpec,
  },
};
use clap::{Args, ValueEnum};
use diagnostics::Diagnostics;
use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt, stream};
use kernel::{KERNELS, Kernel, KernelImageFormat};

const ROOTFS: &str = env!("SAKAI_VMTEST_ROOTFS");
const VERBOSE_KERNEL_COMMAND_LINE: &str =
  "reboot=k panic=-1 panic_print=31 nomodule console=hvc0 rootfstype=virtiofs \
   rw no-kvmapf init=/init.krun loglevel=8 ignore_loglevel initcall_debug \
   printk.time=1";
const SERIAL_KERNEL_COMMAND_LINE: &str =
  "reboot=k panic=-1 panic_print=31 console=ttyS0,115200 console=hvc0 \
   rootfstype=virtiofs rw no-kvmapf init=/init.krun loglevel=8 \
   ignore_loglevel initcall_debug printk.time=1 earlycon=uart,io,0x3f8,115200";

#[derive(Debug, Args)]
pub(crate) struct Vmtest {
  /// Boot BoxLite's bundled kernel and run a minimal guest command.
  #[arg(long)]
  bundled_kernel_smoke: bool,
  /// Boot a selected kernel with a minimal guest command and chosen kernel arguments.
  #[arg(long, value_enum)]
  boot_probe: Option<BootProbe>,
  /// Select the ELF vmlinux or compressed bzImage custom-kernel loader.
  #[arg(long, value_enum, default_value_t = KernelImageFormat::Elf)]
  kernel_image_format: KernelImageFormat,
  /// Inspect selected kernel images without launching BoxLite.
  #[arg(long)]
  inspect_kernel: bool,
  /// Build selected kernels with BoxLite's boot drivers built in.
  #[arg(long)]
  build_kernel: bool,
  /// Copy only BoxLite log files into a safe artifact staging directory.
  #[arg(long)]
  collect_diagnostics: bool,
  /// Choose the quick, complete, or host-native matrix.
  #[arg(long, value_enum, default_value_t = Profile::Smoke)]
  profile: Profile,
  /// Run only the named kernel; repeat to select more than one.
  #[arg(long)]
  kernel: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Profile {
  Smoke,
  Full,
  Native,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BootProbe {
  Default,
  Verbose,
  Serial,
}

impl BootProbe {
  fn command_line(self) -> Option<&'static str> {
    match self {
      | Self::Default => None,
      | Self::Verbose => Some(VERBOSE_KERNEL_COMMAND_LINE),
      | Self::Serial => Some(SERIAL_KERNEL_COMMAND_LINE),
    }
  }
}

impl Profile {
  fn matches(&self, kernel: &Kernel) -> bool {
    match self {
      | Profile::Smoke => kernel.smoke,
      | Profile::Full => true,
      | Profile::Native => kernel.sha256.is_none(),
    }
  }
}

impl Vmtest {
  pub(crate) async fn run(self) -> Result<()> {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .parent()
      .context("xtask must be in the repository root")?
      .canonicalize()?;
    if self.bundled_kernel_smoke {
      return Self::run_bundled_kernel_smoke(&repository).await;
    }
    if self.collect_diagnostics {
      return Diagnostics::collect(&repository);
    }
    let selected = KERNELS
      .iter()
      .filter(|kernel| match self.kernel.is_empty() {
        | false => self.kernel.iter().any(|name| name == kernel.name),
        | true => {
          kernel.architecture == env::consts::ARCH
            && self.profile.matches(kernel)
        },
      })
      .collect::<Vec<_>>();
    ensure!(
      !selected.is_empty(),
      "no kernel fixtures matched the request"
    );

    if self.build_kernel {
      for kernel in selected {
        kernel
          .build(&repository.join("tests/.cache/sakai-vmtest/kernels"))
          .await?;
      }
      return Ok(());
    }

    if self.inspect_kernel {
      for kernel in selected {
        for format in [KernelImageFormat::Elf, KernelImageFormat::Compressed] {
          if let Some(path) = kernel
            .image(
              &repository.join("tests/.cache/sakai-vmtest/kernels"),
              format,
            )
            .await?
          {
            Kernel::report_boot_config(&path)?;
          }
        }
      }
      return Ok(());
    }

    stream::iter(selected)
      .then(|kernel| {
        Self::run_kernel(
          &repository,
          kernel,
          self.boot_probe,
          self.kernel_image_format,
        )
      })
      .try_collect::<Vec<_>>()
      .await
      .map(|_| ())
  }

  async fn run_bundled_kernel_smoke(repository: &Path) -> Result<()> {
    let boxlite_home =
      repository.join("tests/.cache/sakai-vmtest/boxlite/bundled-smoke");
    fs::create_dir_all(&boxlite_home)?;
    boxlite::init_logging_for(&boxlite_home)?;

    eprintln!("==> BoxLite bundled-kernel smoke test");
    let runtime = RuntimeBuilder::new(BoxliteOptions {
      home_dir: boxlite_home,
      ..Default::default()
    })
    .build()?;
    let boxlite = runtime
      .create(
        BoxOptions {
          cpus: Some(1),
          memory_mib: Some(1536),
          disk_size_gb: Some(2),
          rootfs: RootfsSpec::Image(ROOTFS.into()),
          auto_delete: Some(1),
          cmd: Some(["uname", "-r"].map(String::from).into()),
          ..Default::default()
        },
        None,
      )
      .await?;
    let mut execution =
      boxlite.attach(AttachOptions::main().read_only()).await?;
    let stdout = execution
      .stdout()
      .context("BoxLite did not provide stdout")?;
    let stderr = execution
      .stderr()
      .context("BoxLite did not provide stderr")?;
    boxlite.start().await?;
    let (_, _, result) = tokio::try_join!(
      Self::forward(stdout, io::stdout()),
      Self::forward(stderr, io::stderr()),
      execution.wait().map_err(anyhow::Error::from),
    )?;
    runtime.shutdown(None).await?;
    ensure!(
      result.success(),
      "BoxLite bundled-kernel smoke test failed with exit code {}{}",
      result.exit_code,
      result
        .error_message
        .map(|message| format!(": {message}"))
        .unwrap_or_default(),
    );

    Ok(())
  }

  async fn run_kernel(
    repository: &Path,
    kernel: &Kernel,
    boot_probe: Option<BootProbe>,
    format: KernelImageFormat,
  ) -> Result<()> {
    let cache = repository.join("tests/.cache/sakai-vmtest");
    let architecture = kernel.architecture;
    let boxlite_home =
      cache
        .join("boxlite")
        .join(architecture)
        .join(if boot_probe.is_some() {
          "boot-probe"
        } else {
          "test"
        });
    fs::create_dir_all(&boxlite_home)?;
    boxlite::init_logging_for(&boxlite_home)?;

    eprintln!(
      "==> Linux {} ({architecture}), boot_probe={boot_probe:?}, \
       image={format:?}",
      kernel.name
    );
    let runtime = RuntimeBuilder::new(BoxliteOptions {
      home_dir: boxlite_home,
      ..Default::default()
    })
    .enable(ExperimentalFeature::CustomKernel)
    .build()?;
    let mut advanced = AdvancedBoxOptions::default();
    advanced.set_capabilities(Some(ContainerCapabilities {
      add: vec!["SYS_ADMIN".into()],
      ..Default::default()
    }))?;
    let mut options = BoxOptions {
      cpus: Some(1),
      // Clean uom builds were killed at 1 GiB; cached builds need much less.
      memory_mib: Some(1536),
      disk_size_gb: Some(2),
      working_dir: Some("/workspace".into()),
      env: vec![
        ("SAKAI_VMTEST".into(), "1".into()),
        (
          "CARGO_HOME".into(),
          format!("/workspace/tests/.cache/sakai-vmtest/cargo/{architecture}"),
        ),
        (
          "CARGO_TARGET_DIR".into(),
          format!("/workspace/tests/.cache/sakai-vmtest/target/{architecture}"),
        ),
        ("CARGO_HTTP_TIMEOUT".into(), "600".into()),
        ("CARGO_HTTP_LOW_SPEED_LIMIT".into(), "1".into()),
      ],
      rootfs: RootfsSpec::Image(ROOTFS.into()),
      volumes: vec![VolumeSpec::bind_mount(
        repository
          .to_str()
          .context("repository path must be valid UTF-8")?,
        "/workspace",
      )],
      auto_delete: Some(1),
      advanced,
      cmd: Some(match boot_probe {
        | Some(_) => ["uname", "-r"].map(String::from).into(),
        | None => [
          "cargo",
          "test",
          "--locked",
          "--package",
          "sakai-core",
          "--test",
          "linux_live",
          "--",
          "--nocapture",
        ]
        .map(String::from)
        .into(),
      }),
      ..Default::default()
    };
    if let Some(path) = kernel.image(&cache.join("kernels"), format).await? {
      Kernel::report_boot_config(&path)?;
      let kernel_options = match boot_probe.and_then(BootProbe::command_line) {
        | Some(command_line) => {
          eprintln!("Diagnostic kernel command line: {command_line}");
          KernelOptions::new(path).with_command_line(command_line)
        },
        | None => KernelOptions::new(path),
      };
      custom_kernel::configure(&mut options, kernel_options);
    }

    let boxlite = runtime.create(options, None).await?;
    let mut execution =
      boxlite.attach(AttachOptions::main().read_only()).await?;
    let stdout = execution
      .stdout()
      .context("BoxLite did not provide stdout")?;
    let stderr = execution
      .stderr()
      .context("BoxLite did not provide stderr")?;
    boxlite.start().await?;
    let (_, _, result) = tokio::try_join!(
      Self::forward(stdout, io::stdout()),
      Self::forward(stderr, io::stderr()),
      execution.wait().map_err(anyhow::Error::from),
    )?;
    runtime.shutdown(None).await?;
    ensure!(
      result.success(),
      "Linux {} VM test failed with exit code {}{}",
      kernel.name,
      result.exit_code,
      result
        .error_message
        .map(|message| format!(": {message}"))
        .unwrap_or_default(),
    );

    Ok(())
  }

  async fn forward(
    mut stream: impl Stream<Item = String> + Unpin,
    mut output: impl Write,
  ) -> Result<()> {
    while let Some(chunk) = stream.next().await {
      output.write_all(chunk.as_bytes())?;
    }
    Ok(())
  }
}
