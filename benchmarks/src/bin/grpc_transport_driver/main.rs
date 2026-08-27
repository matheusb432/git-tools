mod environment;
mod measurement;
mod run;
mod server;

fn main() {
    let result = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(anyhow::Error::from)
        .and_then(|runtime| runtime.block_on(run::run()));
    if let Err(error) = result {
        eprintln!("gRPC transport benchmark failed: {error:#}");
        std::process::exit(1);
    }
}
