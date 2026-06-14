//! git-tools - CLI entry point: clap parsing + a machine-readable exit-code contract.

use crate::cli::{Cli, Command, DiffTarget, ManagedArgs};
use crate::commands::managed::{ManagedExit, ManagedOptions, ManagedRun};
use crate::commands::squash_local::{SquashResult, Status, StdGitRunner, invoke_squash_local};

pub mod cli;
pub mod commands;
pub mod diff;
pub mod git;
pub mod model;
pub mod open;
pub mod render;

/// Process exit codes. Stable contract every caller (and justfile shim) depends on.
/// Extend with command-specific codes as the tool grows (keep 0/1/2 stable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Ok = 0,
    Internal = 1,
    Usage = 2,
}

fn squash_local_exit_code(status: Status) -> ExitCode {
    match status {
        Status::Refused | Status::Fail => ExitCode::Internal,
        Status::Noop | Status::WouldSquash | Status::Squashed => ExitCode::Ok,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputStream {
    Stdout,
    Stderr,
}

fn squash_local_output_stream(status: Status) -> OutputStream {
    match status {
        Status::Refused | Status::Fail => OutputStream::Stderr,
        Status::Noop | Status::WouldSquash | Status::Squashed => OutputStream::Stdout,
    }
}

/// Route argv (already stripped of argv[0]) to an [`ExitCode`].
pub fn run(args: &[String]) -> ExitCode {
    match Cli::parse_args(args) {
        Ok(cli) => dispatch(cli.command),
        Err(error) => render_clap_error(&error),
    }
}

/// Prints a clap parse outcome and maps it to an exit code: help/version are successes,
/// everything else is a usage error.
fn render_clap_error(error: &clap::Error) -> ExitCode {
    use clap::error::ErrorKind;

    // clap routes help/version to stdout and genuine usage errors to stderr.
    let _ = error.print();
    match error.kind() {
        ErrorKind::DisplayHelp
        | ErrorKind::DisplayVersion
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => ExitCode::Ok,
        _ => ExitCode::Usage,
    }
}

fn dispatch(command: Command) -> ExitCode {
    match command {
        Command::SquashPreview { repo, monorepo } => {
            html_exit(commands::squash_preview::run(repo, monorepo))
        }
        Command::Diff { target } => html_exit(commands::diff::run(&DiffTarget::from_arg(
            target.as_deref(),
        ))),
        Command::DiffSubrepos {
            repo,
            monorepo,
            base,
        } => html_exit(commands::diff_subrepos::run(
            repo,
            monorepo,
            base.as_deref(),
        )),
        Command::MergeDiff {
            repo,
            monorepo,
            base,
        } => html_exit(commands::merge_diff::run(repo, monorepo, base.as_deref())),
        Command::SquashLocal { repo, message, dry } => {
            let runner = StdGitRunner;
            let result = invoke_squash_local(&runner, repo, Some(&message), dry);
            print_squash_local_result(&result, &message);
            squash_local_exit_code(result.status)
        }
        Command::PushAll(args) => managed_exit(commands::managed::run_push_all(&managed_options(
            args, None,
        ))),
        Command::PullAll(args) => managed_exit(commands::managed::run_pull_all(&managed_options(
            args, None,
        ))),
        Command::CommitAll {
            managed,
            message_for_all,
        } => managed_exit(commands::managed::run_commit_all(&managed_options(
            managed,
            message_for_all,
        ))),
    }
}

/// Builds the [`ManagedOptions`] for a fan-out command from its parsed flags.
fn managed_options(args: ManagedArgs, message_for_all: Option<String>) -> ManagedOptions {
    ManagedOptions {
        repos_file: args.repos_file.map(Into::into),
        home_dir: args.home_dir.map(Into::into),
        dry: args.dry,
        json: args.json,
        message_for_all,
        interactive: is_interactive(),
    }
}

fn managed_exit<T>(run: ManagedRun<T>) -> ExitCode {
    if !run.stdout.is_empty() {
        println!("{}", run.stdout);
    }
    if !run.stderr.is_empty() {
        eprintln!("{}", run.stderr);
    }
    match run.exit {
        ManagedExit::Clean => ExitCode::Ok,
        ManagedExit::Warn => ExitCode::Internal,
        ManagedExit::Fail | ManagedExit::Usage => ExitCode::Usage,
    }
}

fn is_interactive() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdin())
}

fn html_exit(result: anyhow::Result<std::path::PathBuf>) -> ExitCode {
    match result {
        Ok(_) => ExitCode::Ok,
        Err(error) => {
            eprintln!("{}", html_error_text(&error));
            ExitCode::Internal
        }
    }
}

fn html_error_text(error: &anyhow::Error) -> String {
    format!("{error:#}")
}

fn print_squash_local_result(result: &SquashResult, message: &str) {
    let stream = squash_local_output_stream(result.status);
    match result.status {
        Status::Refused => print_squash_local_line(stream, &format!("refused: {}", result.detail)),
        Status::Fail => print_squash_local_line(stream, &result.detail),
        Status::Noop => print_squash_local_line(stream, &result.detail),
        Status::WouldSquash => {
            print_squash_local_line(
                stream,
                &format!(
                    "[dry] would collapse {} unpushed commits into one:",
                    result.count
                ),
            );
            for commit in &result.commits {
                print_squash_local_line(stream, &format!("  {commit}"));
            }
            print_squash_local_line(stream, &format!("[dry] new message would be: {message}"));
            print_squash_local_line(stream, "[dry] re-run without --dry to apply.");
        }
        Status::Squashed => {
            print_squash_local_line(
                stream,
                &format!("squashed {} commits into one.", result.count),
            );
            print_squash_local_line(stream, &format!("recover: git reset --soft {}", result.pre));
        }
    }
}

fn print_squash_local_line(stream: OutputStream, line: &str) {
    match stream {
        OutputStream::Stdout => println!("{line}"),
        OutputStream::Stderr => eprintln!("{line}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_values_are_stable() {
        assert_eq!(ExitCode::Ok as i32, 0);
        assert_eq!(ExitCode::Internal as i32, 1);
        assert_eq!(ExitCode::Usage as i32, 2);
    }

    #[test]
    fn no_args_and_help_exit_ok() {
        assert_eq!(run(&[]), ExitCode::Ok);
        assert_eq!(run(&["--help".into()]), ExitCode::Ok);
    }

    #[test]
    fn version_flag_exits_ok() {
        assert_eq!(run(&["--version".into()]), ExitCode::Ok);
    }

    #[test]
    fn blank_commit_all_message_is_usage() {
        assert_eq!(
            run(&["commit-all".into(), "--message-for-all".into(), "".into()]),
            ExitCode::Usage
        );
    }

    #[test]
    fn unknown_command_is_usage() {
        assert_eq!(run(&["bogus".into()]), ExitCode::Usage);
    }

    #[test]
    fn html_errors_print_without_cli_prefix() {
        let error = anyhow::anyhow!("fatal: bad ref\nnot a commit: nope");

        assert_eq!(
            html_error_text(&error),
            "fatal: bad ref\nnot a commit: nope"
        );
    }

    #[test]
    fn squash_local_statuses_map_to_ps1_exit_codes() {
        use commands::squash_local::Status;

        assert_eq!(squash_local_exit_code(Status::Refused), ExitCode::Internal);
        assert_eq!(squash_local_exit_code(Status::Fail), ExitCode::Internal);
        assert_eq!(squash_local_exit_code(Status::Noop), ExitCode::Ok);
        assert_eq!(squash_local_exit_code(Status::WouldSquash), ExitCode::Ok);
        assert_eq!(squash_local_exit_code(Status::Squashed), ExitCode::Ok);
    }

    #[test]
    fn squash_local_output_streams_route_failures_to_stderr() {
        use commands::squash_local::Status;

        assert_eq!(
            squash_local_output_stream(Status::Refused),
            OutputStream::Stderr
        );
        assert_eq!(
            squash_local_output_stream(Status::Fail),
            OutputStream::Stderr
        );
        assert_eq!(
            squash_local_output_stream(Status::Noop),
            OutputStream::Stdout
        );
        assert_eq!(
            squash_local_output_stream(Status::WouldSquash),
            OutputStream::Stdout
        );
        assert_eq!(
            squash_local_output_stream(Status::Squashed),
            OutputStream::Stdout
        );
    }
}
