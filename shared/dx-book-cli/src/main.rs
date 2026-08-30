use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{config::Configuration, serve::ServeArguments};

mod config;
mod serve;

#[derive(Parser)]
#[command(
    name = "dx-book",
    version,
    about = "Development tooling for Dioxus component books",
    styles = clap_cargo::style::CLAP_STYLING
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the configured component book with Rust and stylesheet watchers.
    Serve(ServeArguments),
    /// Generate the configured component-book stylesheet.
    Styles,
}

fn main() {
    let command = Cli::parse().command;
    if let Err(error) = run(command) {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}

fn run(command: Command) -> Result<()> {
    let configuration = Configuration::load()?;
    match command {
        Command::Serve(arguments) => serve::run(configuration, &arguments),
        Command::Styles => serve::build_styles(configuration),
    }
}
