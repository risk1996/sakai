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
use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt, stream};
use kernel::{KERNELS, Kernel};

const ROOTFS: &str = env!("SAKAI_VMTEST_ROOTFS");

#[derive(Debug, Args)]
pub(crate) struct Vmtest {
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

    stream::iter(selected)
      .then(|kernel| Self::run_kernel(&repository, kernel))
      .try_collect::<Vec<_>>()
      .await
      .map(|_| ())
  }

  async fn run_kernel(repository: &Path, kernel: &Kernel) -> Result<()> {
    let cache = repository.join("tests/.cache/sakai-vmtest");
    let architecture = kernel.architecture;
    let boxlite_home = cache.join("boxlite").join(architecture);
    fs::create_dir_all(&boxlite_home)?;
    boxlite::init_logging_for(&boxlite_home)?;

    eprintln!("==> Linux {} ({architecture})", kernel.name);
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
      cmd: Some(
        [
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
      ),
      ..Default::default()
    };
    if let Some(path) = kernel.image(&cache.join("kernels")).await? {
      custom_kernel::configure(&mut options, KernelOptions::new(path));
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
