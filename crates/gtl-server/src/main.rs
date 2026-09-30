use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gtl-server", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(
        subcommand,
        about = "Run a one-shot data snapshot operation for `gtl data`."
    )]
    Data(gtl_server::data::DataCommand),
}

fn main() -> anyhow::Result<std::process::ExitCode> {
    match Cli::parse().command {
        None => run_daemon().map(|()| std::process::ExitCode::SUCCESS),
        Some(Command::Data(command)) => Ok(gtl_server::data::run(command)),
    }
}

#[tokio::main]
async fn run_daemon() -> anyhow::Result<()> {
    gtl_server::run().await
}
