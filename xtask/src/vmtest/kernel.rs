use std::{
  fmt, fs,
  io::Write,
  path::{Path, PathBuf},
  time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result};
use fs2::FileExt;
use reqwest::{Client, StatusCode, header::RETRY_AFTER};
#[derive(Debug)]
pub(super) struct Kernel {
  pub(super) name: &'static str,
  pub(super) smoke: bool,
}

/// Server-requested retry interval retained as HTTP error context.
#[derive(Debug, PartialEq, Eq)]
struct RetryAfter(Duration);

impl RetryAfter {
  fn parse(value: &str, now: SystemTime) -> Option<Self> {
    let value = value.trim();
    let delay = match value.bytes().all(|byte| byte.is_ascii_digit()) {
      | true => Duration::from_secs(value.parse().ok()?),
      | false => httpdate::parse_http_date(value).ok()?.duration_since(now).unwrap_or(Duration::ZERO),
    };
    // Treat intervals that cannot be represented by the timer as unusable.
    Instant::now().checked_add(delay)?;
    Some(Self(delay))
  }

  fn delay(error: &anyhow::Error, fallback: Duration) -> Duration {
    error.downcast_ref::<Self>().map_or(fallback, |retry| retry.0)
  }
}

impl fmt::Display for RetryAfter {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "server requested retry after {:?}", self.0)
  }
}

impl Kernel {
  pub(super) const ALL: &[Self] = &[
    Self::new("5.15", true),
    Self::new("6.1", false),
    Self::new("6.6", false),
    Self::new("6.12", false),
    Self::new("6.18", true),
  ];
  const ASSET_URL: &str = "https://github.com/danobi/vmtest/releases/download/test_assets";
  const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
  const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
  const MAX_DOWNLOAD_ATTEMPTS: u8 = 3;
  const RETRY_DELAY: Duration = Duration::from_secs(2);

  const fn new(name: &'static str, smoke: bool) -> Self { Self { name, smoke } }

  pub(super) async fn image(&self, cache: &Path) -> Result<PathBuf> {
    fs::create_dir_all(cache)?;
    let file_name = format!("bzImage-v{}-archlinux", self.name);
    let destination = cache.join(&file_name);
    let lock = fs::File::create(cache.join(format!("{file_name}.lock")))?;
    lock.lock_exclusive()?;
    if destination.try_exists()? {
      return Ok(destination);
    }
    let url = format!("{}/{file_name}", Self::ASSET_URL);
    let partial = cache.join(format!("{file_name}.{}.partial", std::process::id()));
    let result = async {
      let client = Client::builder().connect_timeout(Self::CONNECT_TIMEOUT).timeout(Self::DOWNLOAD_TIMEOUT).build()?;
      Self::download(&client, &url, &partial, Self::RETRY_DELAY).await?;
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

  async fn download(client: &Client, url: &str, partial: &Path, retry_delay: Duration) -> Result<()> {
    for attempt in 1..=Self::MAX_DOWNLOAD_ATTEMPTS {
      match Self::download_once(client, url, partial).await {
        | Ok(()) => return Ok(()),
        | Err(error) if attempt < Self::MAX_DOWNLOAD_ATTEMPTS && Self::retryable(&error) => {
          eprintln!("kernel download attempt {attempt}/{} failed: {error:#}; retrying", Self::MAX_DOWNLOAD_ATTEMPTS);
          tokio::time::sleep(RetryAfter::delay(&error, retry_delay)).await;
        },
        | Err(error) => {
          return Err(error).with_context(|| {
            format!("kernel download failed on attempt {attempt}/{}: {url}", Self::MAX_DOWNLOAD_ATTEMPTS,)
          });
        },
      }
    }
    anyhow::bail!("kernel download exhausted retries: {url}")
  }

  async fn download_once(client: &Client, url: &str, partial: &Path) -> Result<()> {
    let response = client.get(url).send().await?;
    let retry_after = match response.status() {
      | StatusCode::TOO_MANY_REQUESTS => response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| RetryAfter::parse(value, SystemTime::now())),
      | _ => None,
    };
    let mut response = response.error_for_status().map_err(|error| match retry_after {
      | Some(retry) => anyhow::Error::new(error).context(retry),
      | None => error.into(),
    })?;
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
          || error.status().is_some_and(|status| status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS)
      },
      | None => false,
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant, SystemTime},
  };

  use anyhow::{Context, Result, ensure};
  use reqwest::Client;
  use tempfile::tempdir;

  use super::{Kernel, RetryAfter};

  #[test]
  fn parses_retry_after_and_preserves_fallback() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_445_412_480);
    for (value, expected) in [
      ("10", Some(RetryAfter(Duration::from_secs(10)))),
      (" 0 ", Some(RetryAfter(Duration::ZERO))),
      ("Wed, 21 Oct 2015 07:28:10 GMT", Some(RetryAfter(Duration::from_secs(10)))),
      ("Wed, 21 Oct 2015 07:28:00 GMT", Some(RetryAfter(Duration::ZERO))),
      ("Wed, 21 Oct 2015 07:27:00 GMT", Some(RetryAfter(Duration::ZERO))),
      ("", None),
      ("-1", None),
      ("+10", None),
      ("1.5", None),
      ("invalid", None),
      ("18446744073709551615", None),
      ("18446744073709551616", None),
    ] {
      assert_eq!(RetryAfter::parse(value, now), expected);
    }
    let fallback = Duration::from_secs(2);
    let error = anyhow::anyhow!("missing or invalid Retry-After");
    assert_eq!(RetryAfter::delay(&error, fallback), fallback);
    let error = error.context(RetryAfter(Duration::from_secs(10)));
    assert_eq!(RetryAfter::delay(&error, fallback), Duration::from_secs(10));
  }

  #[tokio::test]
  async fn retries_transient_download_failures_with_a_fixed_budget() -> Result<()> {
    for (responses, expected_body, expected_requests) in [
      (vec![("503 Service Unavailable", "kernel"), ("200 OK", "kernel")], Some("kernel"), 2),
      (vec![("429 Too Many Requests", "kernel"), ("200 OK", "kernel")], Some("kernel"), 2),
      (vec![("429 Too Many Requests\r\nRetry-After: 1", "kernel"), ("200 OK", "kernel")], Some("kernel"), 2),
      (vec![("200 OK", "par"), ("200 OK", "kernel")], Some("kernel"), 2),
      (vec![("503 Service Unavailable", "kernel"); 3], None, 3),
      (vec![("404 Not Found", "kernel")], None, 1),
    ] {
      let server_delay = match responses.iter().any(|(status, _)| status.contains("Retry-After: 1")) {
        | true => Duration::from_secs(1),
        | false => Duration::ZERO,
      };
      let listener = TcpListener::bind("127.0.0.1:0")?;
      listener.set_nonblocking(true)?;
      let url = format!("http://{}/kernel", listener.local_addr()?);
      let server = thread::spawn(move || -> Result<usize> {
        for (status, body) in &responses {
          let started = Instant::now();
          let (mut stream, _) = loop {
            match listener.accept() {
              | Ok(connection) => break connection,
              | Err(error) if error.kind() == ErrorKind::WouldBlock && started.elapsed() < Duration::from_secs(5) => {
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
            ensure!(request.read_line(&mut line)? > 0, "incomplete HTTP request");
            if line == "\r\n" {
              break;
            }
          }
          write!(stream, "HTTP/1.1 {status}\r\nContent-Length: 6\r\nConnection: close\r\n\r\n{body}")?;
        }
        Ok(responses.len())
      });
      let directory = tempdir()?;
      let partial = directory.path().join("kernel.partial");
      fs::write(&partial, "stale incomplete response")?;
      let client = Client::builder().timeout(Duration::from_secs(5)).build()?;
      let started = Instant::now();
      let result = Kernel::download(&client, &url, &partial, Duration::ZERO).await;
      ensure!(started.elapsed() >= server_delay, "server's Retry-After interval was ignored");
      ensure!(result.is_ok() == expected_body.is_some(), "unexpected download result: {result:?}");
      if let Some(expected) = expected_body {
        ensure!(fs::read_to_string(&partial)? == expected, "retry did not replace partial download");
      }
      let requests = server.join().map_err(|payload| anyhow::anyhow!("server panicked: {payload:?}"))??;
      ensure!(requests == expected_requests, "unexpected request count");
    }
    Ok(())
  }

  #[tokio::test]
  async fn retries_connection_failures_but_not_filesystem_errors() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}/kernel", listener.local_addr()?);
    drop(listener);
    let directory = tempdir()?;
    let client = Client::builder().timeout(Duration::from_secs(5)).build()?;
    let result = Kernel::download(&client, &url, &directory.path().join("kernel.partial"), Duration::ZERO).await;
    let error = result.err().context("closed port must exhaust the retry budget")?;
    ensure!(error.to_string().contains("attempt 3/3"), "retry budget not exhausted");
    ensure!(Kernel::retryable(&error), "connection error must be retryable");
    ensure!(
      !Kernel::retryable(&std::io::Error::from(ErrorKind::PermissionDenied).into()),
      "filesystem errors must not be retried"
    );
    Ok(())
  }
}
