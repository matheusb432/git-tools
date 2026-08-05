//! git-tools - CLI entry point: clap parsing + a machine-readable exit-code contract.

use std::path::{Path, PathBuf};

use gtl_application::{
    managed::plan_push::{self, PlanPush, PlanPushOk},
    ports::UserSettingsStore,
};
use gtl_infra::git_client::HybridGitClient;

use crate::{
    cli::{
        Cli, ColorChoice, Command, CommitArgs, DaemonArgs, DaemonCommand, DiffArgs, DiffSub,
        DiffTarget, DiffTargetArgs, ManagedArgs, ManagedReadArgs, MergeArgs, PruneArgs, PushArgs,
        SquashArgs, StatusArgs, SwitchArgs, Theme, WorktreeCommand,
    },
    commands::managed::{ManagedExit, ManagedOptions, ManagedRun, PushOutcome, PushSummary},
    confirm::{Confirmation, DefaultAnswer, RealConfirm},
};

pub mod cli;
pub mod client;
pub mod commands;
pub mod preprocess;
pub(crate) mod recipe;
pub mod viewer;

mod confirm;
mod diff_viewer_client;

/// Process exit codes. Stable contract every caller (and justfile shim) depends on.
/// Extend with command-specific codes as the tool grows (keep 0/1/2 stable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Ok = 0,
    Internal = 1,
    Usage = 2,
}

/// Route argv (already stripped of argv[0]) to an [`ExitCode`].
pub fn run(args: &[String]) -> ExitCode {
    let args = preprocess::normalize(args.to_vec());
    match Cli::parse_args(&args) {
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
        Command::Diff(args) => run_diff(args),
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
        }) => {
            let settings = gtl_infra::user_config::TomlSettingsStore::from_environment();
            match settings.load() {
                Ok(settings) => run_push_current(yes, settings.push_confirmation_required()),
                Err(error) => {
                    eprintln!("error: {error:#}");
                    ExitCode::Internal
                }
            }
        }
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
        Command::Switch(args) => run_switch(&args),
        Command::Prune(args) => run_prune(args),
        Command::Tag(args) => commands::tag::run(args.command, args.commits, args.state),
        Command::Worktree(args) => run_worktree(&args.command),
        Command::Status(args) => managed_exit(&run_status(args)),
        Command::Ls(args) => managed_exit(&run_status(args.into())),
        Command::Daemon(DaemonArgs { command }) => run_daemon_ctl(&command),
    }
}

fn run_diff(args: DiffArgs) -> ExitCode {
    match args.sub {
        Some(DiffSub::Merge(MergeArgs {
            repo_path,
            base,
            raw,
        })) => diff_exit(commands::merge_diff::run(repo_path, base.as_deref(), raw)),
        Some(DiffSub::Squash(SquashArgs { repo_path, raw })) => {
            diff_exit(commands::squash_preview::run(repo_path, raw))
        }
        Some(DiffSub::Live(args)) => diff_live_exit(commands::diff_live::run(args.path)),
        None => {
            let raw = args.raw;
            if let Some(theme) = args.target.set_theme {
                return run_set_theme(theme);
            }
            if args.target.recursive {
                return diff_exit(commands::canonical_working_directory().and_then(|root| {
                    commands::diff_subrepos::run_scan(
                        root,
                        args.target.last,
                        args.target.worktrees,
                        raw,
                    )
                }));
            }
            match diff_invocation(args.target) {
                DiffInvocation::Single { target, name } => {
                    diff_exit(commands::diff::run(&target, name.as_deref(), raw))
                }
                DiffInvocation::ManagedAll {
                    repos_file,
                    home_dir,
                } => {
                    let options = managed_diff_options(repos_file, home_dir);
                    diff_exit(commands::canonical_working_directory().and_then(|root| {
                        commands::diff_subrepos::run_managed_all(root, &options, raw)
                    }))
                }
            }
        }
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

/// Dispatch resident daemon lifecycle commands.
fn run_daemon_ctl(command: &DaemonCommand) -> ExitCode {
    use crate::commands::daemon_ctl;

    let result = match command {
        DaemonCommand::Status => daemon_ctl::status(),
        DaemonCommand::Restart => daemon_ctl::restart(),
        DaemonCommand::Stop => daemon_ctl::stop(),
    };
    match result {
        Ok(()) => ExitCode::Ok,
        Err(error) => {
            eprintln!("gtl-daemon: {error:#}");
            ExitCode::Internal
        }
    }
}

/// Persist the diff-preview theme to the user config and exit (no rendering).
fn run_set_theme(theme: Theme) -> ExitCode {
    let mut store = gtl_infra::user_config::TomlSettingsStore::from_environment();
    let Some(path) = store.path().map(Path::to_path_buf) else {
        eprintln!(
            "error: could not resolve a config path (no GIT_TOOLS_CONFIG, XDG_CONFIG_HOME, or HOME)"
        );
        return ExitCode::Internal;
    };
    let value_new = theme.as_config_str().to_owned();

    match gtl_application::settings::set_key::execute(
        gtl_application::settings::set_key::SetSettingKey {
            key: "theme".into(),
            value_new: value_new.clone(),
        },
        &mut store,
    ) {
        Ok(_) => {
            println!(
                "diff-preview theme set to \"{value_new}\" in {}",
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
    use gtl_application::worktrees::{
        get_base::{self, GetWorktreeBase, GetWorktreeBaseOk},
        list::{self, ListWorktrees, ListWorktreesOk},
    };

    use crate::commands::worktree;

    let git = HybridGitClient;
    let repo_path = match commands::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("worktree: {error:#}");
            return ExitCode::Internal;
        }
    };
    match command {
        WorktreeCommand::Base => match get_base::execute(GetWorktreeBase { repo_path }, &git) {
            Ok(GetWorktreeBaseOk::Found { path }) => {
                if !path.is_empty() {
                    println!("{path}");
                }
                ExitCode::Ok
            }
            Ok(GetWorktreeBaseOk::Failed { detail }) => {
                eprintln!("worktree: {detail}");
                ExitCode::Internal
            }
            Err(error) => {
                eprintln!("worktree: {error}");
                ExitCode::Internal
            }
        },
        WorktreeCommand::Ls => match list::execute(ListWorktrees { repo_path }, &git) {
            Ok(ListWorktreesOk::Listed { worktrees }) => {
                let detail = worktree::render_list(&worktrees);
                if !detail.is_empty() {
                    println!("{detail}");
                }
                ExitCode::Ok
            }
            Ok(ListWorktreesOk::Failed { detail }) => {
                eprintln!("worktree: {detail}");
                ExitCode::Internal
            }
            Err(error) => {
                eprintln!("worktree: {error}");
                ExitCode::Internal
            }
        },
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
        DiffTarget::Unpushed { pinned: None }
    } else if let Some(base) = args.merge {
        DiffTarget::Merge { base, pinned: None }
    } else {
        match args.last {
            Some(count) => DiffTarget::Last {
                count,
                pinned: None,
            },
            None => match args.target {
                None => DiffTarget::Unpushed { pinned: None },
                Some(value) if value.trim().is_empty() => DiffTarget::Unpushed { pinned: None },
                Some(range) if range.contains("..") => DiffTarget::Range {
                    range,
                    pinned: None,
                },
                Some(base) => DiffTarget::Base(base),
            },
        }
    }
}

fn run_push_managed(args: PushArgs) -> ExitCode {
    let PushArgs {
        message,
        dry,
        json,
        repos_file,
        home_dir,
        ..
    } = args;
    let repos_file = repos_file.map(Into::into);
    let home_dir = home_dir.map(Into::into);
    let interactive = confirm::stdin_is_terminal();
    let dry = match plan_push::execute(PlanPush { message, dry }) {
        PlanPushOk::PushOnly { dry } => dry,
        PlanPushOk::CommitThenPush { message, dry } => {
            let run = commands::managed::run_commit_all(&ManagedOptions {
                repos_file: repos_file.clone(),
                home_dir: home_dir.clone(),
                dry,
                json,
                color: false,
                message_for_all: Some(message),
                interactive,
            });
            let exit = managed_exit(&run);
            if exit != ExitCode::Ok {
                return exit;
            }
            dry
        }
    };

    managed_exit(&commands::managed::run_push_all(&ManagedOptions {
        repos_file,
        home_dir,
        dry,
        json,
        color: false,
        message_for_all: None,
        interactive,
    }))
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
/// `--yes`/TTY, then stage, commit, and push.
fn run_push_with_message(message: &str, yes: bool) -> ExitCode {
    use gtl_application::repository_sync::{apply_push, plan_push};

    use crate::commands::sync;

    if message.trim().is_empty() {
        eprintln!("push: a non-empty commit message is required");
        return ExitCode::Usage;
    }

    let git = HybridGitClient;
    let repo_path = match commands::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("push: {error:#}");
            return ExitCode::Internal;
        }
    };
    let target = match plan_push::execute(plan_push::PlanPush { repo_path }, &git) {
        Ok(plan_push::PlanPushOk::Refused(detail)) => {
            eprintln!("push: {detail}");
            return ExitCode::Internal;
        }
        Ok(plan_push::PlanPushOk::Ready(target)) => target,
        Err(error) => {
            eprintln!("push: {error}");
            return ExitCode::Internal;
        }
    };

    println!("{}", sync::confirmation("push", &target, message));

    match confirm::request(&RealConfirm, yes, "Proceed?", DefaultAnswer::Yes) {
        Confirmation::RefuseNonInteractive => {
            eprintln!("push: non-interactive shell; pass --yes to confirm the push");
            return ExitCode::Usage;
        }
        Confirmation::Declined => {
            eprintln!("push: aborted — nothing committed or pushed");
            return ExitCode::Ok;
        }
        Confirmation::Invalid(err) => {
            eprintln!("push: {err} — nothing committed or pushed");
            return ExitCode::Usage;
        }
        Confirmation::Proceed => {}
    }

    let result = match apply_push::execute(
        apply_push::ApplyPush {
            target,
            mode: apply_push::PushMode::CommitChanges {
                message: message.into(),
            },
        },
        &git,
    ) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("push: {error}");
            return ExitCode::Internal;
        }
    };
    match result.status {
        apply_push::PushStatus::Pushed | apply_push::PushStatus::Noop => {
            println!("push: {}", result.detail);
            ExitCode::Ok
        }
        apply_push::PushStatus::Failed | apply_push::PushStatus::Refused => {
            eprintln!("push: {}", result.detail);
            ExitCode::Internal
        }
    }
}

/// Orchestrates current-repo `push` for existing commits only.
fn run_push_current(yes: bool, confirm: bool) -> ExitCode {
    use gtl_application::repository_sync::{apply_push, plan_push};

    use crate::commands::sync;

    let git = HybridGitClient;
    let repo_path = match commands::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("push: {error:#}");
            return ExitCode::Internal;
        }
    };
    let target = match plan_push::execute(plan_push::PlanPush { repo_path }, &git) {
        Ok(plan_push::PlanPushOk::Refused(detail)) => {
            eprintln!("push: {detail}");
            return ExitCode::Internal;
        }
        Ok(plan_push::PlanPushOk::Ready(target)) => target,
        Err(error) => {
            eprintln!("push: {error}");
            return ExitCode::Internal;
        }
    };

    if confirm {
        println!("{}", sync::push_confirmation(&target));
    }

    match confirm::request(
        &RealConfirm,
        yes || !confirm,
        "Proceed?",
        DefaultAnswer::Yes,
    ) {
        Confirmation::RefuseNonInteractive => {
            eprintln!("push: non-interactive shell; pass --yes to confirm the push");
            return ExitCode::Usage;
        }
        Confirmation::Declined => {
            eprintln!("push: aborted — nothing pushed");
            return ExitCode::Ok;
        }
        Confirmation::Invalid(err) => {
            eprintln!("push: {err} — nothing pushed");
            return ExitCode::Usage;
        }
        Confirmation::Proceed => {}
    }

    let result = match apply_push::execute(
        apply_push::ApplyPush {
            target,
            mode: apply_push::PushMode::ExistingOnly,
        },
        &git,
    ) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("push: {error}");
            return ExitCode::Internal;
        }
    };

    match result.status {
        apply_push::PushStatus::Pushed | apply_push::PushStatus::Noop => {
            println!("push: {}", result.detail);
            ExitCode::Ok
        }
        apply_push::PushStatus::Failed | apply_push::PushStatus::Refused => {
            eprintln!("push: {}", result.detail);
            ExitCode::Internal
        }
    }
}

/// Orchestrates current-repo `commit`: review, gate, then stage all changes and create one commit
/// without pushing.
fn run_commit_current(message: &str, yes: bool) -> ExitCode {
    use gtl_application::repository_sync::{apply_commit, plan_commit};

    use crate::commands::sync;

    if message.trim().is_empty() {
        eprintln!("commit: a non-empty commit message is required");
        return ExitCode::Usage;
    }

    let git = HybridGitClient;
    let repo_path = match commands::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("commit: {error:#}");
            return ExitCode::Internal;
        }
    };
    let target = match plan_commit::execute(plan_commit::PlanCommit { repo_path }, &git) {
        Ok(plan_commit::PlanCommitOk::Refused(detail)) => {
            eprintln!("commit: {detail}");
            return ExitCode::Internal;
        }
        Ok(plan_commit::PlanCommitOk::Ready(target)) => target,
        Err(error) => {
            eprintln!("commit: {error}");
            return ExitCode::Internal;
        }
    };

    println!("{}", sync::commit_confirmation(&target, message));

    match confirm::request(&RealConfirm, yes, "Proceed?", DefaultAnswer::Yes) {
        Confirmation::RefuseNonInteractive => {
            eprintln!("commit: non-interactive shell; pass --yes to confirm the commit");
            return ExitCode::Usage;
        }
        Confirmation::Declined => {
            eprintln!("commit: aborted — nothing committed");
            return ExitCode::Usage;
        }
        Confirmation::Invalid(err) => {
            eprintln!("commit: {err} — nothing committed");
            return ExitCode::Usage;
        }
        Confirmation::Proceed => {}
    }

    let result = match apply_commit::execute(
        apply_commit::ApplyCommit {
            target,
            message: message.into(),
        },
        &git,
    ) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("commit: {error}");
            return ExitCode::Internal;
        }
    };
    match result.status {
        apply_commit::CommitStatus::Committed | apply_commit::CommitStatus::Noop => {
            println!("commit: {}", result.detail);
            ExitCode::Ok
        }
        apply_commit::CommitStatus::Failed => {
            eprintln!("commit: {}", result.detail);
            ExitCode::Internal
        }
    }
}

/// Orchestrates recursive `push -r`: discover every repo under the current directory, show the
/// confirmation listing each repo's push destination, gate on `--yes`/TTY like
/// `push "<message>"`, then
/// push. Discovery and the resolved destinations are read-only and local — no fetch. The
/// interactive prompt is the only side effect kept out of the
/// [`gtl_application::push_subrepos`] slice.
fn run_push_subrepos(yes: bool) -> ExitCode {
    use gtl_application::push_subrepos;
    use gtl_models::managed::push_subrepos::SubreposPlan;

    use crate::commands::push_subrepos::confirmation;

    let git = HybridGitClient;
    let root = match commands::canonical_working_directory() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("push -r: {error:#}");
            return ExitCode::Internal;
        }
    };

    let targets = match push_subrepos::plan::execute(
        push_subrepos::plan::PlanPush { root: root.clone() },
        &gtl_infra::repo_discovery::WalkdirRepoDiscovery,
        &git,
    ) {
        Ok(SubreposPlan::Ready(targets)) => targets,
        Ok(SubreposPlan::Refused(detail)) => {
            eprintln!("push -r: {detail}");
            return ExitCode::Internal;
        }
        Err(error) => {
            eprintln!("push -r: {error:#}");
            return ExitCode::Internal;
        }
    };

    println!("{}", confirmation(&root, &targets));

    match confirm::request(&RealConfirm, yes, "Proceed?", DefaultAnswer::Yes) {
        Confirmation::RefuseNonInteractive => {
            eprintln!("push -r: non-interactive shell; pass --yes to confirm the push");
            return ExitCode::Usage;
        }
        Confirmation::Declined => {
            println!("push -r: aborted — nothing pushed");
            return ExitCode::Ok;
        }
        Confirmation::Invalid(err) => {
            eprintln!("push -r: {err} — nothing pushed");
            return ExitCode::Usage;
        }
        Confirmation::Proceed => {}
    }

    let result = push_subrepos::apply::execute(push_subrepos::apply::ApplyPush { targets }, &git);
    let detail = format_push_subrepos_result(&result);
    match result.status {
        gtl_models::managed::push_subrepos::Status::Ok => {
            println!("{detail}");
            ExitCode::Ok
        }
        gtl_models::managed::push_subrepos::Status::Partial
        | gtl_models::managed::push_subrepos::Status::Fail => {
            eprintln!("{detail}");
            ExitCode::Internal
        }
    }
}

fn format_push_subrepos_result(
    result: &gtl_models::managed::push_subrepos::PushAllResult,
) -> String {
    use gtl_models::managed::push_subrepos::{RepoOutcome, Status};

    let summary = PushSummary::from_outcomes(
        result.reports.iter().map(|report| match &report.outcome {
            RepoOutcome::Pushed => PushOutcome::Pushed,
            RepoOutcome::UpToDate | RepoOutcome::Skipped(_) => PushOutcome::Skipped,
            RepoOutcome::Failed(_) => PushOutcome::Failed,
        }),
        false,
    );
    let exit_code = match result.status {
        Status::Ok => 0,
        Status::Partial | Status::Fail => 1,
    };
    let mut detail = summary.render(exit_code);
    for report in &result.reports {
        let line = match &report.outcome {
            RepoOutcome::Pushed => format!("\n  {}: pushed", report.label),
            RepoOutcome::UpToDate => format!("\n  {}: already up to date", report.label),
            RepoOutcome::Skipped(reason) => format!("\n  {}: skipped — {reason}", report.label),
            RepoOutcome::Failed(reason) => format!("\n  {}: failed — {reason}", report.label),
        };
        detail.push_str(&line);
    }
    detail
}

/// Orchestrates `switch`: pick the flow from flags, run the plan read-only, then apply.
/// All git work is local; refusals go to stderr (exit 1), logs to stdout (exit 0).
fn run_switch(args: &SwitchArgs) -> ExitCode {
    let repo_path = match canonical_working_directory_or_exit("switch") {
        Ok(path) => path,
        Err(code) => return code,
    };
    run_switch_with_path(args, repo_path)
}

fn run_switch_with_path(args: &SwitchArgs, repo_path: PathBuf) -> ExitCode {
    use gtl_application::branches::{
        apply_rebase::{self, ApplyRebase, RebaseStatus},
        apply_revert::{self, ApplyRevert, RevertStatus},
        apply_switch::{self, ApplySwitch, SwitchStatus},
        plan_rebase::{self, PlanRebase, PlanRebaseOk},
        plan_revert::{self, PlanRevert, PlanRevertOk},
        plan_switch::{self, PlanSwitch, PlanSwitchOk},
    };

    let git = HybridGitClient;
    let onto = args.onto.as_deref().unwrap_or("main");

    if args.revert {
        return match plan_revert::execute(
            PlanRevert {
                repo_path: repo_path.clone(),
                onto: onto.into(),
            },
            &git,
        ) {
            Ok(PlanRevertOk::Refused(detail)) => {
                eprintln!("switch: {detail}");
                ExitCode::Internal
            }
            Ok(PlanRevertOk::Ready(target)) => {
                let result = match apply_revert::execute(ApplyRevert { target }, &git) {
                    Ok(result) => result,
                    Err(error) => return finish_switch_error(&error),
                };
                finish_switch(match result.status {
                    RevertStatus::Reverted => Ok(&result.detail),
                    RevertStatus::Failed => Err(&result.detail),
                })
            }
            Err(error) => finish_switch_error(&error),
        };
    }

    if args.rebase {
        let target = match plan_rebase::execute(
            PlanRebase {
                repo_path: repo_path.clone(),
                onto: onto.into(),
            },
            &git,
        ) {
            Ok(PlanRebaseOk::Refused(detail)) => {
                eprintln!("switch: {detail}");
                return ExitCode::Internal;
            }
            Ok(PlanRebaseOk::Noop(detail)) => {
                println!("{detail}");
                return ExitCode::Ok;
            }
            Ok(PlanRebaseOk::Ready(target)) => target,
            Err(error) => return finish_switch_error(&error),
        };
        let result = match apply_rebase::execute(ApplyRebase { target }, &git) {
            Ok(result) => result,
            Err(error) => return finish_switch_error(&error),
        };
        let code = finish_switch(match result.status {
            RebaseStatus::FastForwarded => Ok(&result.detail),
            RebaseStatus::Failed => Err(&result.detail),
        });
        if code == ExitCode::Ok && args.diff {
            return diff_exit(commands::diff::run(
                &DiffTarget::Unpushed { pinned: None },
                None,
                false,
            ));
        }
        return code;
    }

    match plan_switch::execute(
        PlanSwitch {
            repo_path,
            onto: onto.into(),
        },
        &git,
    ) {
        Ok(PlanSwitchOk::Refused(detail)) => {
            eprintln!("switch: {detail}");
            ExitCode::Internal
        }
        Ok(PlanSwitchOk::AlreadyThere(onto)) => {
            println!("already on '{onto}'");
            ExitCode::Ok
        }
        Ok(PlanSwitchOk::Ready(target)) => {
            let result = match apply_switch::execute(ApplySwitch { target }, &git) {
                Ok(result) => result,
                Err(error) => return finish_switch_error(&error),
            };
            finish_switch(match result.status {
                SwitchStatus::Switched => Ok(&result.detail),
                SwitchStatus::Failed => Err(&result.detail),
            })
        }
        Err(error) => finish_switch_error(&error),
    }
}

fn finish_switch_error(error: &impl std::fmt::Display) -> ExitCode {
    eprintln!("switch: {error}");
    ExitCode::Internal
}

/// Print an applied `switch` result and map its status to an exit code.
fn finish_switch(result: Result<&str, &str>) -> ExitCode {
    match result {
        Ok(detail) => {
            println!("{detail}");
            ExitCode::Ok
        }
        Err(detail) => {
            eprintln!("switch: {detail}");
            ExitCode::Internal
        }
    }
}

/// Orchestrates `prune`: `--all` fans out over managed repos (preview unless `-y`); the
/// single-repo path plans read-only, shows the will-delete block, gates on `-y`/TTY like
/// `push "<message>"`, then deletes. All git work is local; refusals → stderr, logs → stdout.
fn run_prune(args: PruneArgs) -> ExitCode {
    use gtl_application::branches::{
        apply_prune::{self, ApplyPrune, ApplyPruneError, PruneStatus},
        plan_prune::{self, PlanPrune, PlanPruneOk},
    };

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
            interactive: confirm::stdin_is_terminal(),
        };
        return managed_exit(&commands::managed::run_prune_all(onto, &options));
    }

    let git = HybridGitClient;
    let repo_path = match commands::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("prune: {error:#}");
            return ExitCode::Internal;
        }
    };

    let (top, branches) = match plan_prune::execute(
        PlanPrune {
            repo_path,
            onto: onto.into(),
        },
        &git,
    ) {
        Ok(PlanPruneOk::Refused(detail)) => {
            eprintln!("prune: {detail}");
            return ExitCode::Internal;
        }
        Ok(PlanPruneOk::Nothing(detail)) => {
            println!("{detail}");
            return ExitCode::Ok;
        }
        Ok(PlanPruneOk::Ready { top, branches, .. }) => (top, branches),
        Err(error) => {
            eprintln!("prune: {error:#}");
            return ExitCode::Internal;
        }
    };

    println!("{}", prune::confirmation(onto, &branches));

    match confirm::request(&RealConfirm, args.yes, "Proceed?", DefaultAnswer::Yes) {
        Confirmation::RefuseNonInteractive => {
            eprintln!("prune: non-interactive shell; pass --yes to confirm the deletion");
            return ExitCode::Usage;
        }
        Confirmation::Declined => {
            println!("prune: aborted — nothing deleted");
            return ExitCode::Ok;
        }
        Confirmation::Invalid(err) => {
            eprintln!("prune: {err} — nothing deleted");
            return ExitCode::Usage;
        }
        Confirmation::Proceed => {}
    }

    match apply_prune::execute(ApplyPrune { top, branches }, &git) {
        Ok(result) => {
            let detail = prune::render_result(&result);
            match result.status {
                PruneStatus::Ok => {
                    println!("{detail}");
                    ExitCode::Ok
                }
                PruneStatus::Partial | PruneStatus::Fail => {
                    eprintln!("prune: {detail}");
                    ExitCode::Internal
                }
            }
        }
        Err(error) => {
            if let ApplyPruneError::Transport {
                completed_result: Some(result),
                ..
            } = &error
            {
                println!("{}", prune::render_result(result));
            }
            eprintln!("prune: {error:#}");
            ExitCode::Internal
        }
    }
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
        let root = match commands::canonical_working_directory() {
            Ok(root) => root,
            Err(error) => return status_path_error(&error),
        };
        commands::managed::run_status_recursive(&root, &options)
    } else {
        let root = match commands::canonical_working_directory() {
            Ok(root) => root,
            Err(error) => return status_path_error(&error),
        };
        commands::managed::run_status_current(&root, &options)
    }
}

fn status_path_error(error: &anyhow::Error) -> ManagedRun<commands::managed::StatusResult> {
    ManagedRun {
        exit: ManagedExit::Fail,
        results: Vec::new(),
        stdout: String::new(),
        stderr: format!("status: {error:#}"),
    }
}

fn canonical_working_directory_or_exit(command: &str) -> Result<PathBuf, ExitCode> {
    commands::canonical_working_directory().map_err(|error| {
        eprintln!("{command}: {error:#}");
        ExitCode::Internal
    })
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
        interactive: confirm::stdin_is_terminal(),
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

fn stdout_is_terminal() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}

/// Map a [`commands::diff::DiffOutcome`] result to an [`ExitCode`]: either `Ok` variant
/// (an artifact was rendered, or a clean empty-range no-op) is a success. Shared by every
/// render path that produces a `DiffOutcome` — `diff`, `diff -r`, `diff --all`,
/// `diff merge`, and `diff squash`.
fn diff_exit(result: anyhow::Result<commands::diff::DiffOutcome>) -> ExitCode {
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

/// Map a `diff live` result to an [`ExitCode`]: success (a save, or a clean
/// no-managed-repos-unpushed no-op) is `Ok`; a validation rejection or transport
/// failure prints the daemon's own message and exits `Internal`.
fn diff_live_exit(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::Ok,
        Err(error) => {
            eprintln!("{}", html_error_text(&error));
            ExitCode::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target_args(target: Option<&str>) -> DiffTargetArgs {
        DiffTargetArgs {
            all: false,
            unpushed: false,
            target: target.map(str::to_owned),
            last: None,
            recursive: false,
            worktrees: false,
            merge: None,
            name: None,
            repos_file: None,
            home_dir: None,
            set_theme: None,
        }
    }

    #[test]
    fn diff_target_maps_absent_and_blank_positionals_to_unpushed() {
        for target in [None, Some(""), Some("   ")] {
            assert_eq!(
                diff_target(target_args(target)),
                DiffTarget::Unpushed { pinned: None }
            );
        }
    }

    #[test]
    fn diff_target_distinguishes_exact_ranges_from_base_revisions() {
        assert_eq!(
            diff_target(target_args(Some("abc123..def456"))),
            DiffTarget::Range {
                range: "abc123..def456".into(),
                pinned: None,
            }
        );
        assert_eq!(
            diff_target(target_args(Some("abc123"))),
            DiffTarget::Base("abc123".into())
        );
    }

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
        // Hermetic: point at an empty manifest via `--repos-file` so this never reads the
        // real `$HOME` manifest or touches the developer's actual repos (an empty
        // `[[project]]` array parses to zero managed repos, which is a Clean status run).
        let dir = tempfile::tempdir().expect("tempdir");
        let repos_file = dir.path().join("projects.toml");
        std::fs::write(&repos_file, "").expect("write empty manifest");

        let exit = run(&[
            "ls".into(),
            "--repos-file".into(),
            repos_file.display().to_string(),
        ]);

        assert_eq!(exit, ExitCode::Ok);
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
}
