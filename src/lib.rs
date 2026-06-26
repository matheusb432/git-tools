//! git-tools - CLI entry point: clap parsing + a machine-readable exit-code contract.

use crate::{
    cli::{
        Cli, ColorChoice, Command, DiffCommand, DiffTarget, DiffTargetArgs, ManagedArgs,
        ManagedReadArgs, PruneArgs, StatusArgs, SwArgs, TagCommand, UpArgs, UpCommand,
        WorktreeCommand,
    },
    commands::{
        managed::{ManagedExit, ManagedOptions, ManagedRun},
        squash_local::{SquashResult, Status, StdGitRunner, invoke_squash_local},
    },
};

pub mod attribution;
pub mod cli;
pub mod commands;
mod comment_syntax;
pub mod config;
pub mod diff;
pub mod git;
pub mod model;
pub mod render;
pub mod viewer;

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
        Command::SquashPreview { repo, monorepo: _ } => {
            html_exit(commands::squash_preview::run(repo))
        }
        Command::Diff(args) => {
            match args.command {
                Some(DiffCommand::Subrepos(subrepos)) => diff_exit(
                    commands::diff_subrepos::run_scan(".", subrepos.last, subrepos.worktrees),
                ),
                None => match diff_invocation(args.target) {
                    DiffInvocation::Single { target, name } => {
                        diff_exit(commands::diff::run(&target, name.as_deref()))
                    }
                    DiffInvocation::ManagedAll {
                        repos_file,
                        home_dir,
                    } => {
                        let options = managed_diff_options(repos_file, home_dir);
                        html_exit(commands::diff_subrepos::run_managed_all(".", &options))
                    }
                },
            }
        }
        Command::MergeDiff {
            repo,
            monorepo: _,
            base,
        } => html_exit(commands::merge_diff::run(repo, base.as_deref())),
        Command::SquashLocal { repo, message, dry } => {
            let runner = StdGitRunner;
            let result = invoke_squash_local(&runner, repo, Some(&message), dry);
            print_squash_local_result(&result, &message);
            squash_local_exit_code(result.status)
        }
        Command::Up(UpArgs {
            command: Some(UpCommand::Subrepos(sub)),
            ..
        }) => run_up_subrepos(sub.yes),
        Command::Up(UpArgs { message, yes, .. }) => run_up(message.as_deref().unwrap_or(""), yes),
        Command::Sw(args) => run_sw(args),
        Command::Prune(args) => run_prune(args),
        Command::Tag(args) => run_tag(args.command, args.commits),
        Command::Wk(args) => run_worktree(args.command),
        Command::Status(args) => managed_exit(run_status(args)),
        Command::Ls(args) => {
            managed_exit(commands::managed::run_status(&managed_read_options(args)))
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

fn run_worktree(command: WorktreeCommand) -> ExitCode {
    use crate::commands::worktree;

    let runner = StdGitRunner;
    let result = match command {
        WorktreeCommand::Base => worktree::base(&runner, std::path::Path::new(".")),
        WorktreeCommand::Ls => worktree::list(&runner, std::path::Path::new(".")),
    };

    match result.status {
        worktree::Status::Listed => {
            if !result.detail.is_empty() {
                println!("{}", result.detail);
            }
            ExitCode::Ok
        }
        worktree::Status::Fail => {
            eprintln!("wk: {}", result.detail);
            ExitCode::Internal
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffInvocation {
    Single {
        target: DiffTarget,
        name: Option<String>,
    },
    ManagedAll {
        repos_file: Option<String>,
        home_dir: Option<String>,
    },
}

fn diff_invocation(args: DiffTargetArgs) -> DiffInvocation {
    if args.all {
        return DiffInvocation::ManagedAll {
            repos_file: args.repos_file,
            home_dir: args.home_dir,
        };
    }

    let name = args.name.clone();
    DiffInvocation::Single {
        target: diff_target(args),
        name,
    }
}

fn diff_target(args: DiffTargetArgs) -> DiffTarget {
    // ? `-l N` wins via clap conflict guard; `target` is None whenever `last` is Some.
    if args.unpushed {
        DiffTarget::Unpushed
    } else if let Some(base) = args.merge {
        DiffTarget::Merge(base)
    } else {
        match args.last {
            Some(count) => DiffTarget::Last(count),
            None => DiffTarget::from_arg(args.target.as_deref()),
        }
    }
}

fn managed_diff_options(repos_file: Option<String>, home_dir: Option<String>) -> ManagedOptions {
    ManagedOptions {
        repos_file: repos_file.map(Into::into),
        home_dir: home_dir.map(Into::into),
        dry: false,
        json: false,
        color: false,
        message_for_all: None,
        interactive: false,
    }
}

/// Orchestrates `up`: plan read-only, show the confirmation block, gate on `--yes`/TTY,
/// then stage+commit+push. The interactive prompt is the only side effect kept out of
/// [`commands::sync`] so the logic stays unit-testable.
fn run_up(message: &str, yes: bool) -> ExitCode {
    use crate::commands::sync;

    if message.trim().is_empty() {
        eprintln!("up: a non-empty commit message is required");
        return ExitCode::Usage;
    }

    let runner = StdGitRunner;
    let target = match sync::plan(&runner, std::path::Path::new(".")) {
        sync::Plan::Refused(detail) => {
            eprintln!("up: {detail}");
            return ExitCode::Internal;
        }
        sync::Plan::Ready(target) => target,
    };

    println!("{}", sync::confirmation(&target, message));

    match sync::gate(yes, is_interactive()) {
        sync::Gate::RefuseNonInteractive => {
            eprintln!("up: non-interactive shell; pass --yes to confirm the push");
            return ExitCode::Usage;
        }
        sync::Gate::Confirm => match prompt_confirmation() {
            Ok(sync::Answer::Yes) => {}
            Ok(sync::Answer::No) => {
                eprintln!("up: aborted — nothing committed or pushed");
                return ExitCode::Ok;
            }
            Err(err) => {
                eprintln!("up: {err} — nothing committed or pushed");
                return ExitCode::Usage;
            }
        },
        sync::Gate::Proceed => {}
    }

    let result = sync::apply(&runner, &target, message);
    match result.status {
        sync::Status::Synced | sync::Status::Noop => {
            println!("up: {}", result.detail);
            ExitCode::Ok
        }
        sync::Status::Fail | sync::Status::Refused => {
            eprintln!("up: {}", result.detail);
            ExitCode::Internal
        }
    }
}

/// Orchestrates `up subrepos`: discover every repo under the current directory, show the
/// confirmation listing each repo's push destination, gate on `--yes`/TTY like `up`, then
/// push. Discovery and the resolved destinations are read-only and local — no fetch. The
/// interactive prompt is the only side effect kept out of [`commands::up_subrepos`].
fn run_up_subrepos(yes: bool) -> ExitCode {
    use crate::commands::{sync, up_subrepos};

    let runner = StdGitRunner;
    // Canonicalize first (like `diff subrepos`) so repo labels read off real path segments
    // — the root repo is named for its directory, not the bare ".".
    let root = std::fs::canonicalize(".").unwrap_or_else(|_| std::path::PathBuf::from("."));

    let targets = match up_subrepos::plan(&runner, &root) {
        Ok(up_subrepos::SubreposPlan::Ready(targets)) => targets,
        Ok(up_subrepos::SubreposPlan::Refused(detail)) => {
            eprintln!("up subrepos: {detail}");
            return ExitCode::Internal;
        }
        Err(error) => {
            eprintln!("up subrepos: {error:#}");
            return ExitCode::Internal;
        }
    };

    println!("{}", up_subrepos::confirmation(&root, &targets));

    match sync::gate(yes, is_interactive()) {
        sync::Gate::RefuseNonInteractive => {
            eprintln!("up subrepos: non-interactive shell; pass --yes to confirm the push");
            return ExitCode::Usage;
        }
        sync::Gate::Confirm => match prompt_confirmation() {
            Ok(sync::Answer::Yes) => {}
            Ok(sync::Answer::No) => {
                println!("up subrepos: aborted — nothing pushed");
                return ExitCode::Ok;
            }
            Err(err) => {
                eprintln!("up subrepos: {err} — nothing pushed");
                return ExitCode::Usage;
            }
        },
        sync::Gate::Proceed => {}
    }

    let result = up_subrepos::apply(&runner, &targets);
    match result.status {
        up_subrepos::Status::Ok => {
            println!("{}", result.detail);
            ExitCode::Ok
        }
        up_subrepos::Status::Partial | up_subrepos::Status::Fail => {
            eprintln!("{}", result.detail);
            ExitCode::Internal
        }
    }
}

/// Orchestrates `sw`: pick the flow from flags, run the plan read-only, then apply.
/// All git work is local; refusals go to stderr (exit 1), logs to stdout (exit 0).
fn run_sw(args: SwArgs) -> ExitCode {
    use crate::commands::sw;

    let runner = StdGitRunner;
    let onto = args.onto.as_deref().unwrap_or("main");
    let repo = std::path::Path::new(".");

    if args.revert {
        return match sw::plan_revert(&runner, repo, onto) {
            sw::RevertPlan::Refused(detail) => {
                eprintln!("sw: {detail}");
                ExitCode::Internal
            }
            sw::RevertPlan::Ready(target) => finish_sw(sw::apply_revert(&runner, &target)),
        };
    }

    if args.rebase {
        let target = match sw::plan_rebase(&runner, repo, onto) {
            sw::RebasePlan::Refused(detail) => {
                eprintln!("sw: {detail}");
                return ExitCode::Internal;
            }
            sw::RebasePlan::Noop(detail) => {
                println!("{detail}");
                return ExitCode::Ok;
            }
            sw::RebasePlan::Ready(target) => target,
        };
        let code = finish_sw(sw::apply_rebase(&runner, &target));
        if code == ExitCode::Ok && args.diff {
            return diff_exit(commands::diff::run(&DiffTarget::Unpushed, None));
        }
        return code;
    }

    match sw::plan_switch(&runner, repo, onto) {
        sw::SwitchPlan::Refused(detail) => {
            eprintln!("sw: {detail}");
            ExitCode::Internal
        }
        sw::SwitchPlan::AlreadyThere(onto) => {
            println!("already on '{onto}'");
            ExitCode::Ok
        }
        sw::SwitchPlan::Ready { top, onto, from } => finish_sw(sw::apply_switch(
            &runner,
            std::path::Path::new(&top),
            &onto,
            &from,
        )),
    }
}

/// Print an applied `sw` result and map its status to an exit code.
fn finish_sw(result: crate::commands::sw::SwResult) -> ExitCode {
    use crate::commands::sw::Status;
    match result.status {
        Status::Ok | Status::Noop => {
            println!("{}", result.detail);
            ExitCode::Ok
        }
        Status::Refused | Status::Fail => {
            eprintln!("sw: {}", result.detail);
            ExitCode::Internal
        }
    }
}

/// Orchestrates `prune`: `--all` fans out over managed repos (preview unless `-y`); the
/// single-repo path plans read-only, shows the will-delete block, gates on `-y`/TTY like
/// `up`, then deletes. All git work is local; refusals → stderr, logs → stdout.
fn run_prune(args: PruneArgs) -> ExitCode {
    use crate::commands::prune;

    let onto = args.onto.as_deref().unwrap_or("main");

    if args.all {
        let options = ManagedOptions {
            repos_file: args.repos_file.map(Into::into),
            home_dir: args.home_dir.map(Into::into),
            dry: !args.yes,
            json: args.json,
            color: false,
            message_for_all: None,
            interactive: is_interactive(),
        };
        return managed_exit(commands::managed::run_prune_all(onto, &options));
    }

    let runner = StdGitRunner;
    let repo = std::path::Path::new(".");

    let (top, branches) = match prune::plan(&runner, repo, onto) {
        prune::PrunePlan::Refused(detail) => {
            eprintln!("prune: {detail}");
            return ExitCode::Internal;
        }
        prune::PrunePlan::Nothing(detail) => {
            println!("{detail}");
            return ExitCode::Ok;
        }
        prune::PrunePlan::Ready { top, branches, .. } => (top, branches),
    };

    println!(
        "will delete {} branch(es) merged into '{onto}':",
        branches.len()
    );
    for branch in &branches {
        println!("  {}  {}", branch.name, branch.sha);
    }

    use crate::commands::sync;
    match sync::gate(args.yes, is_interactive()) {
        sync::Gate::RefuseNonInteractive => {
            eprintln!("prune: non-interactive shell; pass --yes to confirm the deletion");
            return ExitCode::Usage;
        }
        sync::Gate::Confirm => match prompt_confirmation() {
            Ok(sync::Answer::Yes) => {}
            Ok(sync::Answer::No) => {
                println!("prune: aborted — nothing deleted");
                return ExitCode::Ok;
            }
            Err(err) => {
                eprintln!("prune: {err} — nothing deleted");
                return ExitCode::Usage;
            }
        },
        sync::Gate::Proceed => {}
    }

    let result = prune::apply(&runner, std::path::Path::new(&top), &branches);
    match result.status {
        prune::Status::Ok => {
            println!("{}", result.detail);
            ExitCode::Ok
        }
        prune::Status::Partial | prune::Status::Fail => {
            eprintln!("prune: {}", result.detail);
            ExitCode::Internal
        }
    }
}

fn run_tag(command: Option<TagCommand>, commits: bool) -> ExitCode {
    use crate::commands::tag;

    let runner = StdGitRunner;
    let result = match command {
        Some(TagCommand::Add { tag, message }) => {
            tag::add(&runner, std::path::Path::new("."), &tag, &message)
        }
        Some(TagCommand::Up {
            tag: Some(tag),
            message: Some(message),
        }) => tag::add_and_push(&runner, std::path::Path::new("."), &tag, &message),
        Some(TagCommand::Up {
            tag: None,
            message: None,
        }) => tag::push(&runner, std::path::Path::new(".")),
        Some(TagCommand::Up { .. }) => {
            eprintln!("tag: tag up requires both <tag> and <message> when creating a tag");
            return ExitCode::Usage;
        }
        Some(TagCommand::Ls) | None => tag::list(&runner, std::path::Path::new("."), commits),
    };

    match result.status {
        tag::Status::Created | tag::Status::Listed | tag::Status::Noop | tag::Status::Pushed => {
            if !result.detail.is_empty() {
                println!("{}", result.detail);
            }
            ExitCode::Ok
        }
        tag::Status::Fail => {
            eprintln!("tag: {}", result.detail);
            ExitCode::Internal
        }
    }
}

/// Prompts on stdout and reads an answer from stdin. Defaults to yes: an empty
/// answer (just Enter) proceeds, `y`/`yes` proceeds, `n`/`no` aborts, and any
/// other reply is an [`sync::AnswerErr`] — never a silent yes or no. A read
/// failure is treated as a refusal (`Answer::No`) so unreadable stdin never pushes.
fn prompt_confirmation() -> crate::commands::sync::AnswerResult {
    use std::io::Write;

    use crate::commands::sync::{Answer, parse_answer};

    print!("Proceed? [Y/n] ");
    let _ = std::io::stdout().flush();
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return Ok(Answer::No);
    }
    parse_answer(&input)
}

/// Dispatches `status` by scope: `--all` ⇒ managed manifest, `-r` ⇒ recursive scan of
/// the current directory, default ⇒ the current repo alone.
fn run_status(args: StatusArgs) -> ManagedRun<commands::managed::StatusResult> {
    let StatusArgs {
        all,
        recursive,
        read,
    } = args;
    let options = managed_read_options(read);
    if all {
        commands::managed::run_status(&options)
    } else if recursive {
        commands::managed::run_status_recursive(std::path::Path::new("."), &options)
    } else {
        commands::managed::run_status_current(std::path::Path::new("."), &options)
    }
}

/// Builds the [`ManagedOptions`] for a read-only managed command from its parsed flags.
fn managed_read_options(args: ManagedReadArgs) -> ManagedOptions {
    let color = match args.color {
        ColorChoice::Auto => stdout_is_terminal(),
        ColorChoice::Always => true,
        ColorChoice::Never => false,
    };

    ManagedOptions {
        repos_file: args.repos_file.map(Into::into),
        home_dir: args.home_dir.map(Into::into),
        dry: false,
        json: args.json,
        color,
        message_for_all: None,
        interactive: false,
    }
}

/// Builds the [`ManagedOptions`] for a fan-out command from its parsed flags.
fn managed_options(args: ManagedArgs, message_for_all: Option<String>) -> ManagedOptions {
    ManagedOptions {
        repos_file: args.repos_file.map(Into::into),
        home_dir: args.home_dir.map(Into::into),
        dry: args.dry,
        json: args.json,
        color: false,
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

fn stdout_is_terminal() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}

/// Like [`html_exit`], but for the single `diff` path: an empty range is a clean
/// no-op (the command already warned), not a failure.
fn diff_exit(result: anyhow::Result<commands::diff::DiffOutcome>) -> ExitCode {
    match result {
        Ok(_) => ExitCode::Ok,
        Err(error) => {
            eprintln!("{}", html_error_text(&error));
            ExitCode::Internal
        }
    }
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
    fn prune_help_exits_ok() {
        assert_eq!(run(&["prune".into(), "--help".into()]), ExitCode::Ok);
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
