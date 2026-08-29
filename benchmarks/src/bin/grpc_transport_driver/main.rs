mod report;
mod runner;

use clap::Parser as _;

#[derive(clap::Parser)]
#[command(about = "Measure GTL's release gRPC transport with ghz")]
struct Arguments {
    /// Compare the current result, then atomically replace the local baseline.
    #[arg(long, conflicts_with = "smoke")]
    update: bool,
    /// Run one short correctness workload without reading or writing benchmark reports.
    #[arg(long)]
    smoke: bool,
}

fn main() {
    let arguments = Arguments::parse();
    let mode = if arguments.smoke {
        runner::Mode::Smoke
    } else {
        runner::Mode::Benchmark {
            update: arguments.update,
        }
    };
    let result = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(anyhow::Error::from)
        .and_then(|runtime| runtime.block_on(runner::run(mode)));
    if let Err(error) = result {
        eprintln!("gRPC transport benchmark failed: {error:#}");
        std::process::exit(1);
    }
}
