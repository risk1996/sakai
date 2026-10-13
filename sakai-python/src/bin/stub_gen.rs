//! Generate the committed type stub and re-exports for the Linux Python extension.

#[cfg(target_os = "linux")]
fn main() -> pyo3_stub_gen::Result<()> { _sakai::generate_stubs() }

#[cfg(not(target_os = "linux"))]
fn main() -> pyo3_stub_gen::Result<()> { Err(std::io::Error::other("stub generation requires Linux").into()) }
