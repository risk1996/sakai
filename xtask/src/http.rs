use reqwest::StatusCode;

/// Retry HTTP transport failures, rate limiting, and server errors, not local or integrity failures.
pub(crate) fn retryable(error: &anyhow::Error) -> bool {
  error.downcast_ref::<reqwest::Error>().is_some_and(|error| match error.status() {
    | Some(status) => status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error(),
    | None => error.is_connect() || error.is_request() || error.is_timeout() || error.is_body() || error.is_decode(),
  })
}

#[cfg(test)]
pub(crate) mod tests {
  use std::{
    io::{BufRead, BufReader, ErrorKind, Write},
    net::TcpListener,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
  };

  use anyhow::{Result, ensure};

  pub(crate) enum Response {
    Reply { status: &'static str, body: &'static str, content_length: usize },
    Disconnect,
    Stall(Duration),
  }

  impl From<(&'static str, &'static str)> for Response {
    fn from((status, body): (&'static str, &'static str)) -> Self {
      Self::Reply { status, body, content_length: body.len() }
    }
  }

  /// A scripted loopback server with bounded accept, read, and write operations.
  pub(crate) struct Server {
    pub(crate) url: String,
    worker: JoinHandle<Result<usize>>,
  }

  impl Server {
    const TIMEOUT: Duration = Duration::from_secs(5);

    pub(crate) fn start(path: &str, responses: Vec<Response>) -> Result<Self> {
      let listener = TcpListener::bind("127.0.0.1:0")?;
      listener.set_nonblocking(true)?;
      let url = format!("http://{}{path}", listener.local_addr()?);
      let worker = thread::spawn(move || -> Result<usize> {
        for response in &responses {
          let started = Instant::now();
          let (mut stream, _) = loop {
            match listener.accept() {
              | Ok(connection) => break connection,
              | Err(error) if error.kind() == ErrorKind::WouldBlock && started.elapsed() < Self::TIMEOUT => {
                thread::sleep(Duration::from_millis(1));
              },
              | Err(error) => return Err(error.into()),
            }
          };
          stream.set_nonblocking(false)?;
          stream.set_read_timeout(Some(Self::TIMEOUT))?;
          stream.set_write_timeout(Some(Self::TIMEOUT))?;
          let mut request = BufReader::new(&mut stream);
          loop {
            let mut line = String::new();
            ensure!(request.read_line(&mut line)? > 0, "incomplete HTTP request");
            if line == "\r\n" {
              break;
            }
          }
          match response {
            | Response::Reply { status, body, content_length } => {
              write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n{body}"
              )?;
            },
            | Response::Disconnect => {},
            | Response::Stall(delay) => thread::sleep(*delay),
          }
        }
        Ok(responses.len())
      });
      Ok(Self { url, worker })
    }

    pub(crate) fn finish(self) -> Result<usize> {
      self.worker.join().map_err(|payload| anyhow::anyhow!("server panicked: {payload:?}"))?
    }
  }
}
