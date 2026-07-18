//! git-tools - CLI entry point: clap parsing + a machine-readable exit-code contract.

use std::path::Path;

use application::{
    managed::plan_push::{self, PlanPush, PushPlan},
    ports::UserSettingsStore,
    squash_local::{self, SquashLocal, SquashResult, SquashStatus},
};
use infra::git_runner::StdGitRunner;

use crate::{
    cli::{
        Cli, ColorChoice, Command, CommitArgs, DaemonArgs, DaemonCommand, DiffSub, DiffTarget,
        DiffTargetArgs, LiveArgs, ManagedArgs, ManagedReadArgs, MergeArgs, PruneArgs, PushArgs,
        SquashArgs, StatusArgs, SwArgs, TagCommand, Theme, WorktreeCommand,
    },
    commands::managed::{ManagedExit, ManagedOptions, ManagedRun},
    confirm::{Confirmation, DefaultAnswer, RealConfirm},
};

pub mod cli;
pub mod client;
pub mod commands;
pub mod preprocess;
pub(crate) mod recipe;
pub mod viewer;

mod confirm;

/// Process exit codes. Stable contract every caller (and justfile shim) depends on.
/// Extend with command-specific codes as the tool grows (keep 0/1/2 stable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Ok = 0,
    Internal = 1,
    Usage = 2,
}

fn squash_local_exit_code(status: SquashStatus) -> ExitCode {
    match status {
        SquashStatus::Refused | SquashStatus::Failed => ExitCode::Internal,
        SquashStatus::Noop | SquashStatus::WouldSquash | SquashStatus::Squashed => ExitCode::Ok,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputStream {
    Stdout,
    Stderr,
}

fn squash_local_output_stream(status: SquashStatus) -> OutputStream {
    match status {
        SquashStatus::Refused | SquashStatus::Failed => OutputStream::Stderr,
        SquashStatus::Noop | SquashStatus::WouldSquash | SquashStatus::Squashed => {
            OutputStream::Stdout
        }
    }
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
        Command::Diff(args) => match args.sub {
            Some(DiffSub::Merge(MergeArgs { repo, base, raw })) => {
                diff_exit(commands::merge_diff::run(repo, base.as_deref(), raw))
            }
            Some(DiffSub::Squash(SquashArgs { repo, raw })) => {
                diff_exit(commands::squash_preview::run(repo, raw))
            }
            Some(DiffSub::Live(LiveArgs { path })) => {
                diff_live_exit(commands::diff_live::run(path))
            }
            None => {
                let raw = args.raw;
                if let Some(theme) = args.target.set_theme {
                    return run_set_theme(theme);
                }
                if args.target.recursive {
                    return diff_exit(commands::diff_subrepos::run_scan(
                        ".",
                        args.target.last,
                        args.target.worktrees,
                        raw,
                    ));
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
                        diff_exit(commands::diff_subrepos::run_managed_all(".", &options, raw))
                    }
                }
            }
        },
        Command::SquashLocal { repo, message, dry } => {
            let runner = StdGitRunner;
            match squash_local::execute(
                SquashLocal {
                    repo: repo.into(),
                    message: message.clone(),
                    dry,
                },
                &runner,
            ) {
                Ok(result) => {
                    print_squash_local_result(&result, &message);
                    squash_local_exit_code(result.status)
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::Internal
                }
            }
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
        }) => {
            let settings = infra::user_config::TomlSettingsStore::from_environment().load();
            run_push_current(yes, settings.push_confirmation_required())
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
    let store = infra::user_config::TomlSettingsStore::from_environment();
    let Some(path) = store.path().map(Path::to_path_buf) else {
        eprintln!(
            "error: could not resolve a config path (no GIT_TOOLS_CONFIG, XDG_CONFIG_HOME, or HOME)"
        );
        return ExitCode::Internal;
    };
    let value_new = theme.as_config_str().to_owned();

    match application::settings::set_key::execute(
        application::settings::set_key::SetSettingKey {
            key: "theme".into(),
            value_new: value_new.clone(),
        },
        &store,
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
    use application::worktrees::{
        get_base::{self, GetWorktreeBase, WorktreeBaseResult},
        list::{self, ListWorktrees, WorktreeListResult},
    };

    use crate::commands::worktree;

    let runner = StdGitRunner;
    let repo = std::path::PathBuf::from(".");
    match command {
        WorktreeCommand::Base => match get_base::execute(GetWorktreeBase { repo }, &runner) {
            Ok(WorktreeBaseResult::Found { path }) => {
                if !path.is_empty() {
                    println!("{path}");
                }
                ExitCode::Ok
            }
            Ok(WorktreeBaseResult::Failed { detail }) => {
                eprintln!("wk: {detail}");
                ExitCode::Internal
            }
            Err(error) => {
                eprintln!("wk: {error}");
                ExitCode::Internal
            }
        },
        WorktreeCommand::Ls => match list::execute(ListWorktrees { repo }, &runner) {
            Ok(WorktreeListResult::Listed { worktrees }) => {
                let detail = worktree::render_list(&worktrees);
                if !detail.is_empty() {
                    println!("{detail}");
                }
                ExitCode::Ok
            }
            Ok(WorktreeListResult::Failed { detail }) => {
                eprintln!("wk: {detail}");
                ExitCode::Internal
            }
            Err(error) => {
                eprintln!("wk: {error}");
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
        PushPlan::PushOnly { dry } => dry,
        PushPlan::CommitThenPush { message, dry } => {
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
    use application::repository_sync::{apply_push, plan_push};

    use crate::commands::sync;

    if message.trim().is_empty() {
        eprintln!("push: a non-empty commit message is required");
        return ExitCode::Usage;
    }

    let runner = StdGitRunner;
    let target = match plan_push::execute(plan_push::PlanPush { repo: ".".into() }, &runner) {
        Ok(plan_push::PushPlan::Refused(detail)) => {
            eprintln!("push: {detail}");
            return ExitCode::Internal;
        }
        Ok(plan_push::PushPlan::Ready(target)) => target,
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
        &runner,
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
    use application::repository_sync::{apply_push, plan_push};

    use crate::commands::sync;

    let runner = StdGitRunner;
    let target = match plan_push::execute(plan_push::PlanPush { repo: ".".into() }, &runner) {
        Ok(plan_push::PushPlan::Refused(detail)) => {
            eprintln!("push: {detail}");
            return ExitCode::Internal;
        }
        Ok(plan_push::PushPlan::Ready(target)) => target,
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
        &runner,
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
    use application::repository_sync::{apply_commit, plan_commit};

    use crate::commands::sync;

    if message.trim().is_empty() {
        eprintln!("commit: a non-empty commit message is required");
        return ExitCode::Usage;
    }

    let runner = StdGitRunner;
    let target = match plan_commit::execute(plan_commit::PlanCommit { repo: ".".into() }, &runner) {
        Ok(plan_commit::CommitPlan::Refused(detail)) => {
            eprintln!("commit: {detail}");
            return ExitCode::Internal;
        }
        Ok(plan_commit::CommitPlan::Ready(target)) => target,
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
        &runner,
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
/// [`application::push_subrepos`] slice.
fn run_push_subrepos(yes: bool) -> ExitCode {
    use application::push_subrepos;
    use domain::managed::push_subrepos::SubreposPlan;

    let runner = StdGitRunner;
    // Canonicalize first (like `diff -r`) so repo labels read off real path segments
    // — the root repo is named for its directory, not the bare ".".
    let root = std::fs::canonicalize(".").unwrap_or_else(|_| std::path::PathBuf::from("."));

    let targets = match push_subrepos::plan::execute(
        push_subrepos::plan::PlanPush { root: root.clone() },
        &infra::repo_discovery::WalkdirRepoDiscovery,
        &runner,
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

    println!("{}", push_subrepos::confirmation(&root, &targets));

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

    let result =
        push_subrepos::apply::execute(push_subrepos::apply::ApplyPush { targets }, &runner);
    match result.status {
        domain::managed::push_subrepos::Status::Ok => {
            println!("{}", result.detail);
            ExitCode::Ok
        }
        domain::managed::push_subrepos::Status::Partial
        | domain::managed::push_subrepos::Status::Fail => {
            eprintln!("{}", result.detail);
            ExitCode::Internal
        }
    }
}

/// Orchestrates `sw`: pick the flow from flags, run the plan read-only, then apply.
/// All git work is local; refusals go to stderr (exit 1), logs to stdout (exit 0).
fn run_sw(args: &SwArgs) -> ExitCode {
    use application::branches::{
        apply_rebase::{self, ApplyRebase, RebaseStatus},
        apply_revert::{self, ApplyRevert, RevertStatus},
        apply_switch::{self, ApplySwitch, SwitchStatus},
        plan_rebase::{self, PlanRebase, RebasePlan},
        plan_revert::{self, PlanRevert, RevertPlan},
        plan_switch::{self, PlanSwitch, SwitchPlan},
    };

    let runner = StdGitRunner;
    let onto = args.onto.as_deref().unwrap_or("main");

    if args.revert {
        return match plan_revert::execute(
            PlanRevert {
                repo: ".".into(),
                onto: onto.into(),
            },
            &runner,
        ) {
            Ok(RevertPlan::Refused(detail)) => {
                eprintln!("sw: {detail}");
                ExitCode::Internal
            }
            Ok(RevertPlan::Ready(target)) => {
                let result = match apply_revert::execute(ApplyRevert { target }, &runner) {
                    Ok(result) => result,
                    Err(error) => return finish_sw_error(&error),
                };
                finish_sw(match result.status {
                    RevertStatus::Reverted => Ok(&result.detail),
                    RevertStatus::Failed => Err(&result.detail),
                })
            }
            Err(error) => finish_sw_error(&error),
        };
    }

    if args.rebase {
        let target = match plan_rebase::execute(
            PlanRebase {
                repo: ".".into(),
                onto: onto.into(),
            },
            &runner,
        ) {
            Ok(RebasePlan::Refused(detail)) => {
                eprintln!("sw: {detail}");
                return ExitCode::Internal;
            }
            Ok(RebasePlan::Noop(detail)) => {
                println!("{detail}");
                return ExitCode::Ok;
            }
            Ok(RebasePlan::Ready(target)) => target,
            Err(error) => return finish_sw_error(&error),
        };
        let result = match apply_rebase::execute(ApplyRebase { target }, &runner) {
            Ok(result) => result,
            Err(error) => return finish_sw_error(&error),
        };
        let code = finish_sw(match result.status {
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
            repo: ".".into(),
            onto: onto.into(),
        },
        &runner,
    ) {
        Ok(SwitchPlan::Refused(detail)) => {
            eprintln!("sw: {detail}");
            ExitCode::Internal
        }
        Ok(SwitchPlan::AlreadyThere(onto)) => {
            println!("already on '{onto}'");
            ExitCode::Ok
        }
        Ok(SwitchPlan::Ready(target)) => {
            let result = match apply_switch::execute(ApplySwitch { target }, &runner) {
                Ok(result) => result,
                Err(error) => return finish_sw_error(&error),
            };
            finish_sw(match result.status {
                SwitchStatus::Switched => Ok(&result.detail),
                SwitchStatus::Failed => Err(&result.detail),
            })
        }
        Err(error) => finish_sw_error(&error),
    }
}

fn finish_sw_error(error: &impl std::fmt::Display) -> ExitCode {
    eprintln!("sw: {error}");
    ExitCode::Internal
}

/// Print an applied `sw` result and map its status to an exit code.
fn finish_sw(result: Result<&str, &str>) -> ExitCode {
    match result {
        Ok(detail) => {
            println!("{detail}");
            ExitCode::Ok
        }
        Err(detail) => {
            eprintln!("sw: {detail}");
            ExitCode::Internal
        }
    }
}

/// Orchestrates `prune`: `--all` fans out over managed repos (preview unless `-y`); the
/// single-repo path plans read-only, shows the will-delete block, gates on `-y`/TTY like
/// `push "<message>"`, then deletes. All git work is local; refusals → stderr, logs → stdout.
fn run_prune(args: PruneArgs) -> ExitCode {
    use application::branches::{
        apply_prune::{self, ApplyPrune, ApplyPruneError, PruneStatus},
        plan_prune::{self, PlanPrune, PrunePlan},
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

    let runner = StdGitRunner;

    let (top, branches) = match plan_prune::execute(
        PlanPrune {
            repo: ".".into(),
            onto: onto.into(),
        },
        &runner,
    ) {
        Ok(PrunePlan::Refused(detail)) => {
            eprintln!("prune: {detail}");
            return ExitCode::Internal;
        }
        Ok(PrunePlan::Nothing(detail)) => {
            println!("{detail}");
            return ExitCode::Ok;
        }
        Ok(PrunePlan::Ready { top, branches, .. }) => (top, branches),
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

    match apply_prune::execute(ApplyPrune { top, branches }, &runner) {
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

fn run_tag(command: Option<TagCommand>, commits: bool) -> ExitCode {
    use application::tags::{
        add::{self, AddTag},
        add_and_push::{self, AddAndPushTag},
        label::{self, LabelTag},
        list::{self, ListTags},
        push::{self, PushTags},
    };

    let runner = StdGitRunner;
    let repo = std::path::PathBuf::from(".");
    match command {
        Some(TagCommand::Add { tag, message }) => {
            finish_tag_action(add::execute(AddTag { repo, tag, message }, &runner))
        }
        Some(TagCommand::Up {
            tag: Some(tag),
            message: Some(message),
            label,
        }) => finish_tag_action(add_and_push::execute(
            AddAndPushTag {
                repo,
                tag,
                message,
                label,
            },
            &runner,
        )),
        Some(TagCommand::Up {
            tag: Some(tag),
            message: None,
            label: Some(label),
        }) => finish_tag_action(label::execute(LabelTag { repo, tag, label }, &runner)),
        Some(TagCommand::Up {
            tag: None,
            message: None,
            label: None,
        }) => finish_tag_action(push::execute(PushTags { repo }, &runner)),
        Some(TagCommand::Up { tag: None, .. }) => {
            eprintln!("tag: tag up --label requires a <tag> to label");
            ExitCode::Usage
        }
        Some(TagCommand::Up { .. }) => {
            eprintln!("tag: tag up requires both <tag> and <message> when creating a tag");
            ExitCode::Usage
        }
        Some(TagCommand::Ls) | None => {
            finish_tag_list(list::execute(ListTags { repo }, &runner), commits)
        }
    }
}

fn finish_tag_list<E>(result: Result<application::tags::TagList, E>, commits: bool) -> ExitCode
where
    E: std::fmt::Display,
{
    use application::tags::TagList;

    use crate::commands::tag;

    match result {
        Ok(list @ TagList::Listed { .. }) => {
            let detail = tag::render_list(&list, commits);
            if !detail.is_empty() {
                println!("{detail}");
            }
            ExitCode::Ok
        }
        Ok(TagList::Failed { detail }) => {
            eprintln!("tag: {detail}");
            ExitCode::Internal
        }
        Err(error) => {
            eprintln!("tag: {error}");
            ExitCode::Internal
        }
    }
}

fn finish_tag_action<E>(result: Result<application::tags::TagActionOutcome, E>) -> ExitCode
where
    E: std::fmt::Display,
{
    match result {
        Ok(outcome) => render_tag_action(&outcome),
        Err(error) => {
            eprintln!("tag: {error}");
            ExitCode::Internal
        }
    }
}

fn render_tag_action(outcome: &application::tags::TagActionOutcome) -> ExitCode {
    use application::tags::TagActionStatus;

    match outcome.status {
        TagActionStatus::Created | TagActionStatus::Noop | TagActionStatus::Pushed => {
            if !outcome.detail.is_empty() {
                println!("{}", outcome.detail);
            }
            ExitCode::Ok
        }
        TagActionStatus::Failed => {
            eprintln!("tag: {}", outcome.detail);
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

fn print_squash_local_result(result: &SquashResult, message: &str) {
    let stream = squash_local_output_stream(result.status);
    match result.status {
        SquashStatus::Refused => {
            print_squash_local_line(stream, &format!("refused: {}", result.detail));
        }
        SquashStatus::Failed | SquashStatus::Noop => {
            print_squash_local_line(stream, &result.detail);
        }
        SquashStatus::WouldSquash => {
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
        SquashStatus::Squashed => {
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
        // `[[repo]]` array parses to zero managed repos, which is a Clean status run).
        let dir = tempfile::tempdir().expect("tempdir");
        let repos_file = dir.path().join("repos.toml");
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

    #[test]
    fn squash_local_statuses_map_to_ps1_exit_codes() {
        assert_eq!(
            squash_local_exit_code(SquashStatus::Refused),
            ExitCode::Internal
        );
        assert_eq!(
            squash_local_exit_code(SquashStatus::Failed),
            ExitCode::Internal
        );
        assert_eq!(squash_local_exit_code(SquashStatus::Noop), ExitCode::Ok);
        assert_eq!(
            squash_local_exit_code(SquashStatus::WouldSquash),
            ExitCode::Ok
        );
        assert_eq!(squash_local_exit_code(SquashStatus::Squashed), ExitCode::Ok);
    }

    #[test]
    fn squash_local_output_streams_route_failures_to_stderr() {
        assert_eq!(
            squash_local_output_stream(SquashStatus::Refused),
            OutputStream::Stderr
        );
        assert_eq!(
            squash_local_output_stream(SquashStatus::Failed),
            OutputStream::Stderr
        );
        assert_eq!(
            squash_local_output_stream(SquashStatus::Noop),
            OutputStream::Stdout
        );
        assert_eq!(
            squash_local_output_stream(SquashStatus::WouldSquash),
            OutputStream::Stdout
        );
        assert_eq!(
            squash_local_output_stream(SquashStatus::Squashed),
            OutputStream::Stdout
        );
    }
}
