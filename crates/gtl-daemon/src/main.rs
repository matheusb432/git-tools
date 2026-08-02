//! Thin process root for the `gtl-daemon` binary: build the tokio runtime and
//! delegate to [`gtl_daemon::run`].

fn main() -> std::process::ExitCode {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("gtl-daemon: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match rt.block_on(gtl_daemon::run()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gtl-daemon: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
