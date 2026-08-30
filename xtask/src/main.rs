use anyhow::Result;
use clap::Parser;

mod cli;
mod process;
mod task;
mod verbs;

fn main() {
    verbs::wasm_c::exit_if_adapter();
    if let Err(e) = run(cli::Cli::parse().command) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn run(command: cli::Command) -> Result<()> {
    match command {
        cli::Command::Setup => verbs::setup::run(),
        cli::Command::Install { target } => verbs::install::run_install(target),
        cli::Command::Uninstall {
            remove_config,
            force,
        } => verbs::install::run_uninstall(remove_config, force),
        cli::Command::DesktopE2eWorker => verbs::desktop_e2e::run(),
        cli::Command::DesktopScrollFixture => verbs::desktop_scroll::refresh_fixture(),
        cli::Command::DesktopScrollBenchmark(arguments) => {
            verbs::desktop_scroll::run_benchmark(&arguments)
        }
        cli::Command::ServerHighlightingBenchmark(arguments) => {
            verbs::server_highlighting::run_benchmark(&arguments)
        }
        cli::Command::GrpcTransportBenchmark(arguments) => {
            verbs::grpc_transport::run_benchmark(&arguments)
        }
        cli::Command::GrpcTransportSmoke => verbs::grpc_transport::run_smoke(),
        cli::Command::ServerHighlightingProfile => verbs::server_highlighting::run_profile(),
        cli::Command::DesktopScrollFixtureWorker => verbs::desktop_scroll::run_fixture_worker(),
        cli::Command::DesktopScrollBenchmarkWorker => verbs::desktop_scroll::run_benchmark_worker(),
        cli::Command::E2eRuntimeWorker {
            executable,
            arguments,
        } => verbs::desktop_e2e::run_runtime(&executable, &arguments),
        cli::Command::Build { target } => verbs::build::run(target),
        cli::Command::WebBuild => verbs::dioxus_web::build_release(),
        cli::Command::WebServe { arguments } => verbs::dioxus_web::serve(&arguments),
        cli::Command::WebStyles => verbs::dioxus_web::build_styles(),
        cli::Command::CheckDioxusFormat => verbs::format::check_dioxus(),
        cli::Command::PreCommit => verbs::pre_commit::run(),
        cli::Command::DriftCheck => verbs::drift::run(),
        cli::Command::CheckStructure => {
            verbs::check_structure::run(None);
            Ok(())
        }
        cli::Command::CheckParserWasm => verbs::wasm_c::check_parser(&verbs::repository_root()),
        cli::Command::GenIcon => verbs::icon::run(),
        cli::Command::Ship { smoke, force } => verbs::ship::run(smoke, force),
    }
}
