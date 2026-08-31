use std::{io::Write as _, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;

#[derive(Parser)]
#[command(about = "measure demand-loaded full-context viewer source")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Materialize and inspect fixtures without running timed operations.
    Describe {
        /// JSON descriptor destination.
        #[arg(long)]
        output: PathBuf,
    },
    /// Run one independent timed launch.
    Measure {
        /// One-based launch number in the fixed three-launch protocol.
        #[arg(long)]
        launch: usize,
        /// Exact source commit represented by this measurement.
        #[arg(long)]
        source_commit: String,
        /// Repository-native command that initiated the measurement.
        #[arg(long)]
        invocation: String,
        /// JSON fragment destination.
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("view-source benchmark failed: {error:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Describe { output } => {
            let descriptor = gtl_benchmarks::view_source::describe()?;
            write_json_atomically(&output, &descriptor)?;
            println!("view-source descriptor: {}", output.display());
        }
        Command::Measure {
            launch,
            source_commit,
            invocation,
            output,
        } => {
            let fragment =
                gtl_benchmarks::view_source::measure_launch(launch, source_commit, invocation)?;
            write_json_atomically(&output, &fragment)?;
            println!("view-source launch {launch}: {}", output.display());
        }
    }
    Ok(())
}

fn write_json_atomically(path: &std::path::Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("view-source output has no parent")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create view-source output directory {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "create temporary view-source output in {}",
            parent.display()
        )
    })?;
    serde_json::to_writer_pretty(&mut temporary, value).context("encode view-source output")?;
    temporary
        .write_all(b"\n")
        .context("terminate view-source JSON output")?;
    temporary.flush().context("flush view-source output")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync view-source output")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publish view-source output {}", path.display()))?;
    Ok(())
}
