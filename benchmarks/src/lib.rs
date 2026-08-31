//! Shared support for repository benchmarks.

pub mod desktop_scroll;
pub mod release_server;
pub mod server_highlighting;
pub mod view_source;

/// Returns a benchmark fixture value or terminates the benchmark process with context.
pub fn require<T, Error>(result: Result<T, Error>, context: &str) -> T
where
    Error: std::fmt::Display,
{
    match result {
        Ok(value) => value,
        Err(error) => {
            eprintln!("benchmark setup failed while {context}: {error}");
            std::process::exit(1);
        }
    }
}
