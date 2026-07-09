//! git-tools - CLI entry point: clap parsing + a machine-readable exit-code contract.

use crate::{
    cli::{
        Cli, ColorChoice, Command, CommitArgs, DaemonArgs, DaemonCommand, DiffTarget,
        DiffTargetArgs, ManagedArgs, ManagedReadArgs, PruneArgs, PushArgs, StatusArgs, SwArgs,
        TagCommand, Theme, WorktreeCommand,
    },
    commands::{
        git_runner::StdGitRunner,
        managed::{ManagedExit, ManagedOptions, ManagedRun},
        squash_local::{SquashResult, Status, invoke_squash_local},
    },
};

pub mod cli;
pub mod client;
pub mod commands;
pub mod config;
pub mod viewer;

pub(crate) mod model {
    pub use domain::diffs::Commit;
}

pub(crate) mod git {
    pub use infra::git_capture::*;
}

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
            if let Some(theme) = args.target.set_theme {
                return run_set_theme(theme);
            }
            if args.target.recursive {
                return diff_exit(commands::diff_subrepos::run_scan(
                    ".",
                    args.target.last,
                    args.target.worktrees,
                ));
            }
            match diff_invocation(args.target) {
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
        Command::Push(PushArgs {
            all: false,
            recursive: false,
            message: Some(message),
            yes,
            ..
        }) => run_push_with_message(&message, yes),
        Command::Push(PushArgs {
            all: false,
            recursive: false,
            message: None,
            yes,
            ..
        }) => run_push_current(None, yes),
        Command::Push(PushArgs {
            all: false,
            recursive: true,
            yes,
            ..
        }) => run_push_subrepos(yes),
        Command::Push(args) => run_push_managed(args),
        Command::Pull(args) => managed_exit(&commands::managed::run_pull_all(&managed_options(
            args.managed,
            None,
        ))),
        Command::Commit(args) => run_commit(args),
        Command::Sw(args) => run_sw(&args),
        Command::Prune(args) => run_prune(args),
        Command::Tag(args) => run_tag(args.command, args.commits),
        Command::Wk(args) => run_worktree(&args.command),
        Command::Status(args) => managed_exit(&run_status(args)),
        Command::Ls(args) => managed_exit(&run_status(args.into())),
        Command::Daemon(DaemonArgs { command }) => run_daemon_ctl(&command),
    }
}

fn run_commit(args: CommitArgs) -> ExitCode {
    match args {
        CommitArgs {
            all: false,
            message: Some(message),
            yes,
            ..
        } => run_commit_current(&message, yes),
        CommitArgs {
            all: false,
            message: None,
            ..
        } => {
            eprintln!("commit: a non-empty commit message is required");
            ExitCode::Usage
        }
        CommitArgs {
            all: true,
            message,
            dry,
            json,
            repos_file,
            home_dir,
            ..
        } => managed_exit(&commands::managed::run_commit_all(&managed_options(
            ManagedArgs {
                dry,
                json,
                repos_file,
                home_dir,
            },
            message,
        ))),
    }
}

/// Dispatch `daemon status|stop`. Both verbs exit 0 whether or not a daemon is
/// running.
fn run_daemon_ctl(command: &DaemonCommand) -> ExitCode {
    use crate::commands::daemon_ctl;

    match command {
        DaemonCommand::Status => daemon_ctl::status(),
        DaemonCommand::Stop => daemon_ctl::stop(),
    }
    ExitCode::Ok
}

/// Persist the diff-preview theme to the user config and exit (no rendering).
fn run_set_theme(theme: Theme) -> ExitCode {
    match config::save_theme(theme.as_config_str()) {
        Ok(path) => {
            println!(
                "diff-preview theme set to \"{}\" in {}",
                theme.as_config_str(),
                path.display()
            );
            ExitCode::Ok
        }
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::Internal
        }
    }
}

fn run_worktree(command: &WorktreeCommand) -> ExitCode {
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ManagedPushStep {
    Commit(ManagedOptions),
    Push(ManagedOptions),
}

fn managed_push_steps(args: PushArgs) -> Vec<ManagedPushStep> {
    let managed = ManagedArgs {
        dry: args.dry,
        json: args.json,
        repos_file: args.repos_file,
        home_dir: args.home_dir,
    };
    let push_options = managed_options(managed.clone(), None);
    match args.message {
        Some(message) => vec![
            ManagedPushStep::Commit(managed_options(managed, Some(message))),
            ManagedPushStep::Push(push_options),
        ],
        None => vec![ManagedPushStep::Push(push_options)],
    }
}

fn run_push_managed(args: PushArgs) -> ExitCode {
    for step in managed_push_steps(args) {
        match step {
            ManagedPushStep::Commit(options) => {
                let exit = managed_exit(&commands::managed::run_commit_all(&options));
                if exit != ExitCode::Ok {
                    return exit;
                }
            }
            ManagedPushStep::Push(options) => {
                return managed_exit(&commands::managed::run_push_all(&options));
            }
        }
    }
    ExitCode::Ok
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

/// Orchestrates `push "<message>"`: plan read-only, show the confirmation block, gate on
/// `--yes`/TTY, then stage+commit+push. The interactive prompt is the only side effect kept
/// out of [`commands::sync`] so the logic stays unit-testable.
fn run_push_with_message(message: &str, yes: bool) -> ExitCode {
    use crate::commands::sync;

    if message.trim().is_empty() {
        eprintln!("push: a non-empty commit message is required");
        return ExitCode::Usage;
    }

    let runner = StdGitRunner;
    let target = match sync::plan(&runner, std::path::Path::new(".")) {
        sync::Plan::Refused(detail) => {
            eprintln!("push: {detail}");
            return ExitCode::Internal;
        }
        sync::Plan::Ready(target) => target,
    };

    println!("{}", sync::confirmation("push", &target, message));

    match sync::gate(yes, is_interactive()) {
        sync::Gate::RefuseNonInteractive => {
            eprintln!("push: non-interactive shell; pass --yes to confirm the push");
            return ExitCode::Usage;
        }
        sync::Gate::Confirm => match prompt_confirmation() {
            Ok(sync::Answer::Yes) => {}
            Ok(sync::Answer::No) => {
                eprintln!("push: aborted — nothing committed or pushed");
                return ExitCode::Ok;
            }
            Err(err) => {
                eprintln!("push: {err} — nothing committed or pushed");
                return ExitCode::Usage;
            }
        },
        sync::Gate::Proceed => {}
    }

    let result = sync::apply(&runner, &target, message);
    match result.status {
        sync::Status::Synced | sync::Status::Noop => {
            println!("push: {}", result.detail);
            ExitCode::Ok
        }
        sync::Status::Fail | sync::Status::Refused => {
            eprintln!("push: {}", result.detail);
            ExitCode::Internal
        }
    }
}

/// Orchestrates current-repo `push`: optionally stage+commit first when a message is supplied,
/// otherwise push existing commits only.
fn run_push_current(message: Option<&str>, yes: bool) -> ExitCode {
    use crate::commands::sync;

    if let Some(message) = message
        && message.trim().is_empty()
    {
        eprintln!("push: a non-empty commit message is required");
        return ExitCode::Usage;
    }

    let runner = StdGitRunner;
    let target = match sync::plan(&runner, std::path::Path::new(".")) {
        sync::Plan::Refused(detail) => {
            eprintln!("push: {detail}");
            return ExitCode::Internal;
        }
        sync::Plan::Ready(target) => target,
    };

    match message {
        Some(message) => println!("{}", sync::confirmation("push", &target, message)),
        None => println!("{}", sync::push_confirmation(&target)),
    }

    match sync::gate(yes, is_interactive()) {
        sync::Gate::RefuseNonInteractive => {
            eprintln!("push: non-interactive shell; pass --yes to confirm the push");
            return ExitCode::Usage;
        }
        sync::Gate::Confirm => match prompt_confirmation() {
            Ok(sync::Answer::Yes) => {}
            Ok(sync::Answer::No) => {
                eprintln!("{}", push_confirmation_outcome(message, false, None));
                return ExitCode::Ok;
            }
            Err(err) => {
                eprintln!(
                    "{}",
                    push_confirmation_outcome(message, true, Some(&err.to_string()))
                );
                return ExitCode::Usage;
            }
        },
        sync::Gate::Proceed => {}
    }

    let result = match message {
        Some(message) => sync::apply(&runner, &target, message),
        None => sync::push_existing(&runner, &target),
    };

    match result.status {
        sync::Status::Synced | sync::Status::Noop => {
            println!("push: {}", result.detail);
            ExitCode::Ok
        }
        sync::Status::Fail | sync::Status::Refused => {
            eprintln!("push: {}", result.detail);
            ExitCode::Internal
        }
    }
}

fn push_confirmation_outcome(message: Option<&str>, invalid: bool, detail: Option<&str>) -> String {
    match (message, invalid, detail) {
        (None, false, _) => "push: aborted — nothing pushed".to_string(),
        (None, true, Some(detail)) => format!("push: {detail} — nothing pushed"),
        (Some(_), false, _) => "push: aborted — nothing committed or pushed".to_string(),
        (Some(_), true, Some(detail)) => {
            format!("push: {detail} — nothing committed or pushed")
        }
        (_, true, None) => "push: invalid confirmation response — nothing pushed".to_string(),
    }
}

/// Orchestrates current-repo `commit`: review, gate, then stage all changes and create one commit
/// without pushing.
fn run_commit_current(message: &str, yes: bool) -> ExitCode {
    use crate::commands::sync;

    if message.trim().is_empty() {
        eprintln!("commit: a non-empty commit message is required");
        return ExitCode::Usage;
    }

    let runner = StdGitRunner;
    let target = match sync::plan_local_commit(&runner, std::path::Path::new(".")) {
        sync::LocalCommitPlan::Refused(detail) => {
            eprintln!("commit: {detail}");
            return ExitCode::Internal;
        }
        sync::LocalCommitPlan::Ready(target) => target,
    };

    println!("{}", sync::commit_confirmation(&target, message));

    match sync::gate(yes, is_interactive()) {
        sync::Gate::RefuseNonInteractive => {
            eprintln!("commit: non-interactive shell; pass --yes to confirm the commit");
            return ExitCode::Usage;
        }
        sync::Gate::Confirm => match prompt_confirmation() {
            Ok(sync::Answer::Yes) => {}
            Ok(sync::Answer::No) => {
                eprintln!("commit: aborted — nothing committed");
                return ExitCode::Usage;
            }
            Err(err) => {
                eprintln!("commit: {err} — nothing committed");
                return ExitCode::Usage;
            }
        },
        sync::Gate::Proceed => {}
    }

    let result = sync::commit_local(&runner, &target, message);
    match result.status {
        sync::Status::Synced | sync::Status::Noop => {
            println!("commit: {}", result.detail);
            ExitCode::Ok
        }
        sync::Status::Fail | sync::Status::Refused => {
            eprintln!("commit: {}", result.detail);
            ExitCode::Internal
        }
    }
}

/// Orchestrates recursive `push -r`: discover every repo under the current directory, show the
/// confirmation listing each repo's push destination, gate on `--yes`/TTY like
/// `push "<message>"`, then
/// push. Discovery and the resolved destinations are read-only and local — no fetch. The
/// interactive prompt is the only side effect kept out of [`commands::up_subrepos`].
fn run_push_subrepos(yes: bool) -> ExitCode {
    use crate::commands::{sync, up_subrepos};

    let runner = StdGitRunner;
    // Canonicalize first (like `diff -r`) so repo labels read off real path segments
    // — the root repo is named for its directory, not the bare ".".
    let root = std::fs::canonicalize(".").unwrap_or_else(|_| std::path::PathBuf::from("."));

    let targets = match up_subrepos::plan(&runner, &root) {
        Ok(up_subrepos::SubreposPlan::Ready(targets)) => targets,
        Ok(up_subrepos::SubreposPlan::Refused(detail)) => {
            eprintln!("push -r: {detail}");
            return ExitCode::Internal;
        }
        Err(error) => {
            eprintln!("push -r: {error:#}");
            return ExitCode::Internal;
        }
    };

    println!("{}", up_subrepos::confirmation(&root, &targets));

    match sync::gate(yes, is_interactive()) {
        sync::Gate::RefuseNonInteractive => {
            eprintln!("push -r: non-interactive shell; pass --yes to confirm the push");
            return ExitCode::Usage;
        }
        sync::Gate::Confirm => match prompt_confirmation() {
            Ok(sync::Answer::Yes) => {}
            Ok(sync::Answer::No) => {
                println!("push -r: aborted — nothing pushed");
                return ExitCode::Ok;
            }
            Err(err) => {
                eprintln!("push -r: {err} — nothing pushed");
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
fn run_sw(args: &SwArgs) -> ExitCode {
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
            sw::RevertPlan::Ready(target) => finish_sw(&sw::apply_revert(&runner, &target)),
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
        let code = finish_sw(&sw::apply_rebase(&runner, &target));
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
        sw::SwitchPlan::Ready { top, onto, from } => finish_sw(&sw::apply_switch(
            &runner,
            std::path::Path::new(&top),
            &onto,
            &from,
        )),
    }
}

/// Print an applied `sw` result and map its status to an exit code.
fn finish_sw(result: &crate::commands::sw::SwResult) -> ExitCode {
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
/// `push "<message>"`, then deletes. All git work is local; refusals → stderr, logs → stdout.
fn run_prune(args: PruneArgs) -> ExitCode {
    use crate::commands::{prune, sync};

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
        return managed_exit(&commands::managed::run_prune_all(onto, &options));
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
            label,
        }) => tag::add_and_push(
            &runner,
            std::path::Path::new("."),
            &tag,
            &message,
            label.as_deref(),
        ),
        Some(TagCommand::Up {
            tag: Some(tag),
            message: None,
            label: Some(label),
        }) => tag::label_tag(&runner, std::path::Path::new("."), &tag, &label),
        Some(TagCommand::Up {
            tag: None,
            message: None,
            label: None,
        }) => tag::push(&runner, std::path::Path::new(".")),
        Some(TagCommand::Up { tag: None, .. }) => {
            eprintln!("tag: tag up --label requires a <tag> to label");
            return ExitCode::Usage;
        }
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

fn managed_exit<T>(run: &ManagedRun<T>) -> ExitCode {
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
        Status::Fail | Status::Noop => print_squash_local_line(stream, &result.detail),
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
    fn blank_managed_commit_message_is_usage() {
        assert_eq!(
            run(&["commit".into(), "--all".into(), String::new()]),
            ExitCode::Usage
        );
    }

    #[test]
    fn ls_exits_ok() {
        assert_eq!(run(&["ls".into()]), ExitCode::Ok);
    }

    #[test]
    fn managed_push_without_message_has_only_push_step() {
        let steps = managed_push_steps(PushArgs {
            message: None,
            all: true,
            recursive: false,
            dry: false,
            json: false,
            repos_file: Some("repos.toml".to_string()),
            home_dir: Some("/tmp/home".to_string()),
            yes: false,
        });

        assert!(matches!(
            steps.as_slice(),
            [ManagedPushStep::Push(options)]
                if options.message_for_all.is_none()
                    && options.repos_file.as_deref() == Some(std::path::Path::new("repos.toml"))
                    && options.home_dir.as_deref() == Some(std::path::Path::new("/tmp/home"))
        ));
    }

    #[test]
    fn managed_push_with_message_commits_then_pushes() {
        let steps = managed_push_steps(PushArgs {
            message: Some("save work".to_string()),
            all: true,
            recursive: false,
            dry: false,
            json: true,
            repos_file: Some("repos.toml".to_string()),
            home_dir: Some("/tmp/home".to_string()),
            yes: false,
        });

        assert!(matches!(
            steps.as_slice(),
            [ManagedPushStep::Commit(commit), ManagedPushStep::Push(push)]
                if commit.message_for_all.as_deref() == Some("save work")
                    && push.message_for_all.is_none()
                    && commit.json
                    && push.json
        ));
    }

    #[test]
    fn prune_help_exits_ok() {
        assert_eq!(run(&["prune".into(), "--help".into()]), ExitCode::Ok);
    }

    #[test]
    fn push_confirmation_outcome_uses_push_only_copy_without_message() {
        assert_eq!(
            push_confirmation_outcome(None, false, None),
            "push: aborted — nothing pushed"
        );
        assert_eq!(
            push_confirmation_outcome(None, true, Some("unrecognized answer")),
            "push: unrecognized answer — nothing pushed"
        );
        assert_eq!(
            push_confirmation_outcome(Some("save work"), false, None),
            "push: aborted — nothing committed or pushed"
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
