use std::{
  fs,
  io::{Read, Write},
  path::{Path, PathBuf},
  time::Duration,
};

use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};

pub(super) const KERNELS: &[Kernel] = &[
  Kernel::new(
    "5.15",
    true,
    "23879f21c7e3c7137902904fd89695fc9d8a938f1d2c1549dd1658443cb7a084",
  ),
  Kernel::new(
    "6.1",
    false,
    "303b9010d92e4a9cf3930114f5c854edb2c5f7d1f9da2c3c29df7b1f7ab86a3c",
  ),
  Kernel::new(
    "6.6",
    false,
    "3b47b1fefe02d49208da139bb1ad0363971ca5c02ab4ce89b9b8a52504b0fedf",
  ),
  Kernel::new(
    "6.12",
    false,
    "a389c774c4bf035fbb7685be8493d128d8196c62862f750a8ef49b3b58428738",
  ),
  Kernel::new(
    "6.18",
    true,
    "c2a05883f9556e73f5665d5979679163d80ff17754c4090e8d801df2a2e92551",
  ),
];

#[derive(Debug)]
pub(super) struct Kernel {
  pub(super) name: &'static str,
  pub(super) smoke: bool,
  pub(super) sha256: &'static str,
}

impl Kernel {
  const fn new(name: &'static str, smoke: bool, sha256: &'static str) -> Self {
    Self {
      name,
      smoke,
      sha256,
    }
  }

  pub(super) async fn image(&self, cache: &Path) -> Result<PathBuf> {
    fs::create_dir_all(cache)?;
    let file_name = format!("bzImage-v{}-archlinux", self.name);
    let destination = cache.join(&file_name);
    let lock = fs::File::create(cache.join(format!("{file_name}.lock")))?;
    lock.lock_exclusive()?;
    if destination.try_exists()? && Self::matches(&destination, self.sha256)? {
      return Ok(destination);
    }
    let url = format!("https://github.com/danobi/vmtest/releases/download/test_assets/{file_name}");
    let partial =
      cache.join(format!("{file_name}.{}.partial", std::process::id()));
    let result = async {
      let client = Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(120))
        .build()?;
      Self::download(&client, &url, &partial, Duration::from_secs(2)).await?;
      ensure!(
        Self::matches(&partial, self.sha256)?,
        "checksum mismatch for {url}"
      );
      fs::rename(&partial, &destination)?;
      Ok::<_, anyhow::Error>(())
    }
    .await;
    if result.is_err() {
      drop(fs::remove_file(&partial));
    }
    result?;
    Ok(destination)
  }

  async fn download(
    client: &Client,
    url: &str,
    partial: &Path,
    retry_delay: Duration,
  ) -> Result<()> {
    for attempt in 1..=3 {
      match Self::download_once(client, url, partial).await {
        | Ok(()) => return Ok(()),
        | Err(error) if attempt < 3 && Self::retryable(&error) => {
          eprintln!(
            "kernel download attempt {attempt}/3 failed: {error:#}; retrying"
          );
          tokio::time::sleep(retry_delay).await;
        },
        | Err(error) => {
          return Err(error).with_context(|| {
            format!("kernel download failed on attempt {attempt}/3: {url}")
          });
        },
      }
    }
    anyhow::bail!("kernel download exhausted retries: {url}")
  }

  async fn download_once(
    client: &Client,
    url: &str,
    partial: &Path,
  ) -> Result<()> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    // Each attempt truncates any incomplete response from the previous one.
    let mut output = fs::File::create(partial)?;
    while let Some(chunk) = response.chunk().await? {
      output.write_all(&chunk)?;
    }
    Ok(())
  }

  fn retryable(error: &anyhow::Error) -> bool {
    match error.downcast_ref::<reqwest::Error>() {
      | Some(error) => {
        error.is_connect()
          || error.is_request()
          || error.is_timeout()
          || error.is_body()
          || error.is_decode()
          || error.status().is_some_and(|status| {
            status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS
          })
      },
      | None => false,
    }
  }

  fn matches(path: &Path, expected: &str) -> Result<bool> {
    Ok(Self::digest(path)? == expected)
  }

  pub(super) fn digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
      match file.read(&mut buffer)? {
        | 0 => break,
        | size => hasher.update(
          buffer
            .get(..size)
            .context("read exceeded checksum buffer")?,
        ),
      }
    }
    Ok(hex::encode(hasher.finalize()))
  }
}

#[cfg(test)]
mod tests {
  use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
  };

  use anyhow::{Context, Result, ensure};
  use reqwest::Client;
  use tempfile::tempdir;

  use super::Kernel;

  #[tokio::test]
  async fn retries_transient_download_failures_with_a_fixed_budget()
  -> Result<()> {
    for (responses, expected_body, expected_requests) in [
      (
        vec![("503 Service Unavailable", "kernel"), ("200 OK", "kernel")],
        Some("kernel"),
        2,
      ),
      (
        vec![("429 Too Many Requests", "kernel"), ("200 OK", "kernel")],
        Some("kernel"),
        2,
      ),
      (
        vec![("200 OK", "par"), ("200 OK", "kernel")],
        Some("kernel"),
        2,
      ),
      (vec![("503 Service Unavailable", "kernel"); 3], None, 3),
      (vec![("404 Not Found", "kernel")], None, 1),
    ] {
      let listener = TcpListener::bind("127.0.0.1:0")?;
      listener.set_nonblocking(true)?;
      let url = format!("http://{}/kernel", listener.local_addr()?);
      let server = thread::spawn(move || -> Result<usize> {
        for (status, body) in &responses {
          let started = Instant::now();
          let (mut stream, _) = loop {
            match listener.accept() {
              | Ok(connection) => break connection,
              | Err(error)
                if error.kind() == ErrorKind::WouldBlock
                  && started.elapsed() < Duration::from_secs(5) =>
              {
                thread::sleep(Duration::from_millis(1))
              },
              | Err(error) => return Err(error.into()),
            }
          };
          stream.set_nonblocking(false)?;
          stream.set_read_timeout(Some(Duration::from_secs(5)))?;
          let mut request = BufReader::new(&mut stream);
          loop {
            let mut line = String::new();
            ensure!(
              request.read_line(&mut line)? > 0,
              "incomplete HTTP request"
            );
            if line == "\r\n" {
              break;
            }
          }
          write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: 6\r\nConnection: \
             close\r\n\r\n{body}"
          )?;
        }
        Ok(responses.len())
      });
      let directory = tempdir()?;
      let partial = directory.path().join("kernel.partial");
      fs::write(&partial, "stale incomplete response")?;
      let client = Client::builder().timeout(Duration::from_secs(5)).build()?;
      let result =
        Kernel::download(&client, &url, &partial, Duration::ZERO).await;
      ensure!(
        result.is_ok() == expected_body.is_some(),
        "unexpected download result: {result:?}"
      );
      if let Some(expected) = expected_body {
        ensure!(
          fs::read_to_string(&partial)? == expected,
          "retry did not replace partial download"
        );
      }
      let requests = server
        .join()
        .map_err(|payload| anyhow::anyhow!("server panicked: {payload:?}"))??;
      ensure!(requests == expected_requests, "unexpected request count");
    }
    Ok(())
  }

  #[tokio::test]
  async fn retries_connection_failures_but_not_filesystem_errors() -> Result<()>
  {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}/kernel", listener.local_addr()?);
    drop(listener);
    let directory = tempdir()?;
    let client = Client::builder().timeout(Duration::from_secs(5)).build()?;
    let result = Kernel::download(
      &client,
      &url,
      &directory.path().join("kernel.partial"),
      Duration::ZERO,
    )
    .await;
    let error = result
      .err()
      .context("closed port must exhaust the retry budget")?;
    ensure!(
      error.to_string().contains("attempt 3/3"),
      "retry budget not exhausted"
    );
    ensure!(
      Kernel::retryable(&error),
      "connection error must be retryable"
    );
    ensure!(
      !Kernel::retryable(
        &std::io::Error::from(ErrorKind::PermissionDenied).into()
      ),
      "filesystem errors must not be retried"
    );
    Ok(())
  }
}
