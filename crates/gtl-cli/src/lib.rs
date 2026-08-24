//! git-tools - CLI entry point: clap parsing + a machine-readable exit-code contract.

use std::path::{Path, PathBuf};

use gtl_models::git::BranchName;
use gtl_wire::v1;

use crate::{
    cli::{
        Cli, ColorChoice, Command, CommitArgs, DiffArgs, DiffSub, DiffTarget, DiffTargetArgs,
        DiffTargetParseError, ManagedArgs, ManagedReadArgs, MergeArgs, PruneArgs, PushArgs,
        ServerArgs, ServerCommand, StatusArgs, SwitchArgs, Theme, WorktreeCommand,
    },
    commands::managed::{
        ManagedExit, ManagedOptions, ManagedOutput, ManagedRun, PushOutcome, PushSummary,
    },
    confirm::{Confirmation, DefaultAnswer, RealConfirm},
    server_client::ServerClient,
};

pub mod cli;
pub mod commands;
pub mod preprocess;
pub mod viewer;

mod confirm;
mod diff_viewer_client;
mod server_client;
#[cfg(test)]
mod testing;

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
        }) => match ServerClient::connect()
            .and_then(|client| client.get_push_confirmation_requirement())
        {
            Ok(settings) => run_push_current(yes, settings.push_confirmation_required),
            Err(error) => {
                eprintln!("error: {}", error_text(&error));
                ExitCode::Internal
            }
        },
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
        Command::Prune(args) => run_prune(&args),
        Command::Tag(args) => commands::tag::run(args.command, args.commits, args.state),
        Command::Worktree(args) => run_worktree(&args.command),
        Command::Status(args) => managed_exit(&run_status(&args)),
        Command::Ls(args) => {
            let args = args.into();
            managed_exit(&run_status(&args))
        }
        Command::Server(ServerArgs { command }) => run_server_ctl(&command),
    }
}

fn run_diff(args: DiffArgs) -> ExitCode {
    match args.sub {
        Some(DiffSub::Merge(MergeArgs {
            repo_path,
            base,
            raw,
        })) => diff_exit(commands::merge_diff::run(repo_path, base.as_deref(), raw)),
        Some(DiffSub::Live(args)) => diff_live_exit(commands::diff_live::run(args.path)),
        None => {
            let raw = args.raw;
            if let Some(theme) = args.target.set_theme {
                return run_set_theme(theme);
            }
            if args.target.scope.recursive {
                return diff_exit(commands::canonical_working_directory().and_then(|root| {
                    commands::diff_subrepos::run_scan(
                        root,
                        args.target.last,
                        args.target.scope.worktrees,
                        raw,
                    )
                }));
            }
            match diff_invocation(args.target) {
                Ok(DiffInvocation::Single { target, name }) => {
                    diff_exit(commands::diff::run(&target, name.as_deref(), raw))
                }
                Ok(DiffInvocation::ManagedAll) => diff_exit(
                    commands::canonical_working_directory()
                        .and_then(|root| commands::diff_subrepos::run_managed_all(root, raw)),
                ),
                Err(error) => {
                    eprintln!("diff: {error}");
                    ExitCode::Usage
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
            managed,
            ..
        } => managed_exit(&commands::managed::run_commit_all(&managed_options(
            managed, message,
        ))),
    }
}

fn run_server_ctl(command: &ServerCommand) -> ExitCode {
    let result = match command {
        ServerCommand::Status => crate::commands::server_ctl::status(),
    };
    match result {
        Ok(()) => ExitCode::Ok,
        Err(error) => {
            eprintln!("gtl-server: {}", error_text(&error));
            ExitCode::Internal
        }
    }
}

/// Persist the diff-artifact theme to the user config and exit (no rendering).
fn run_set_theme(theme: Theme) -> ExitCode {
    let value_new = gtl_models::viewer::Theme::from(theme).to_string();
    let theme = match theme {
        Theme::Dark => v1::ViewerTheme::Dark,
        Theme::Light => v1::ViewerTheme::Light,
        Theme::Hearth => v1::ViewerTheme::Hearth,
        Theme::Mirage => v1::ViewerTheme::Mirage,
        Theme::Glacier => v1::ViewerTheme::Glacier,
        Theme::Noir => v1::ViewerTheme::Noir,
        Theme::Graphite => v1::ViewerTheme::Graphite,
    };
    match ServerClient::connect().and_then(|client| {
        client.set_viewer_theme(v1::SetViewerThemeRequest {
            theme: theme as i32,
        })
    }) {
        Ok(response) => {
            println!(
                "diff-artifact theme set to \"{value_new}\" in {}",
                response.configuration_path
            );
            ExitCode::Ok
        }
        Err(error) => {
            eprintln!("error: {}", error_text(&error));
            ExitCode::Internal
        }
    }
}

fn run_worktree(command: &WorktreeCommand) -> ExitCode {
    use crate::commands::worktree;

    let repo_path = match commands::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("worktree: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let client = match ServerClient::connect() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("worktree: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let repository_path = repo_path.to_string_lossy().into_owned();
    match command {
        WorktreeCommand::Base => {
            match client.get_worktree_base(v1::GetWorktreeBaseRequest { repository_path }) {
                Ok(response) => match response.outcome {
                    Some(v1::get_worktree_base_response::Outcome::Found(found)) => {
                        println!("{}", found.path);
                        ExitCode::Ok
                    }
                    Some(v1::get_worktree_base_response::Outcome::Failed(failed)) => {
                        eprintln!("worktree: {}", failed.detail);
                        ExitCode::Internal
                    }
                    None => {
                        eprintln!("worktree: gtl-server returned no worktree-base outcome");
                        ExitCode::Internal
                    }
                },
                Err(error) => {
                    eprintln!("worktree: {}", error_text(&error));
                    ExitCode::Internal
                }
            }
        }
        WorktreeCommand::Ls => {
            match client.list_worktrees(v1::ListWorktreesRequest { repository_path }) {
                Ok(response) => match response.outcome {
                    Some(v1::list_worktrees_response::Outcome::Listed(listed)) => {
                        let worktrees = match listed
                            .worktrees
                            .into_iter()
                            .map(worktree::from_grpc)
                            .collect::<anyhow::Result<Vec<_>>>()
                        {
                            Ok(worktrees) => worktrees,
                            Err(error) => {
                                eprintln!("worktree: {}", error_text(&error));
                                return ExitCode::Internal;
                            }
                        };
                        let detail = worktree::render_list(&worktrees);
                        if !detail.is_empty() {
                            println!("{detail}");
                        }
                        ExitCode::Ok
                    }
                    Some(v1::list_worktrees_response::Outcome::Failed(failed)) => {
                        eprintln!("worktree: {}", failed.detail);
                        ExitCode::Internal
                    }
                    None => {
                        eprintln!("worktree: gtl-server returned no worktree-list outcome");
                        ExitCode::Internal
                    }
                },
                Err(error) => {
                    eprintln!("worktree: {}", error_text(&error));
                    ExitCode::Internal
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffInvocation {
    Single {
        target: DiffTarget,
        name: Option<String>,
    },
    ManagedAll,
}

fn diff_invocation(args: DiffTargetArgs) -> Result<DiffInvocation, DiffTargetParseError> {
    if args.scope.all {
        return Ok(DiffInvocation::ManagedAll);
    }

    let name = args.name.clone();
    Ok(DiffInvocation::Single {
        target: diff_target(args)?,
        name,
    })
}

fn diff_target(args: DiffTargetArgs) -> Result<DiffTarget, DiffTargetParseError> {
    // ? `-l N` wins via clap conflict guard; `target` is None whenever `last` is Some.
    if args.unpushed {
        Ok(DiffTarget::Unpushed { pinned: None })
    } else if let Some(base) = args.merge {
        Ok(DiffTarget::Merge {
            base: gtl_models::git::GitRevision::try_new(base)
                .map_err(|_| DiffTargetParseError::EmptyRevision)?,
            pinned: None,
        })
    } else {
        match args.last {
            Some(count) => Ok(DiffTarget::Last {
                count,
                pinned: None,
            }),
            None => match args.target {
                None => Ok(DiffTarget::Unpushed { pinned: None }),
                Some(value) if value.trim().is_empty() => Ok(DiffTarget::Unpushed { pinned: None }),
                Some(range) if range.contains("..") => Ok(DiffTarget::Range {
                    range: gtl_models::git::GitRange::try_new(range)
                        .map_err(|_| DiffTargetParseError::EmptyRange)?,
                    pinned: None,
                }),
                Some(rev) => Ok(DiffTarget::Base(
                    gtl_models::git::GitRevision::try_new(rev)
                        .map_err(|_| DiffTargetParseError::EmptyRevision)?,
                )),
            },
        }
    }
}

fn run_push_managed(args: PushArgs) -> ExitCode {
    let PushArgs {
        message,
        managed: ManagedArgs { dry, json },
        ..
    } = args;
    let interactive = confirm::stdin_is_terminal();
    if let Some(message) = message {
        let run = commands::managed::run_commit_for_push_all(&ManagedOptions {
            dry,
            output: ManagedOutput::from_flags(json, false),
            message_for_all: Some(message),
            interactive,
        });
        let exit = managed_exit(&run);
        if exit != ExitCode::Ok {
            return exit;
        }
    }

    managed_exit(&commands::managed::run_push_all(&ManagedOptions {
        dry,
        output: ManagedOutput::from_flags(json, false),
        message_for_all: None,
        interactive,
    }))
}

/// Orchestrates `push "<message>"`: plan read-only, show the confirmation block, gate on
/// `--yes`/TTY, then stage, commit, and push.
fn run_push_with_message(message: &str, yes: bool) -> ExitCode {
    use crate::commands::sync;

    if message.trim().is_empty() {
        eprintln!("push: a non-empty commit message is required");
        return ExitCode::Usage;
    }

    let (client, target) = match plan_current_push() {
        Ok(planned) => planned,
        Err(error) => {
            eprintln!("push: {}", error_text(&error));
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

    let result = match client.execute_repository_push(v1::ExecuteRepositoryPushRequest {
        target: Some(sync::push_target_to_grpc(&target)),
        mode: v1::RepositoryPushMode::CommitChanges as i32,
        message: Some(message.into()),
    }) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("push: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    finish_push_response(&result)
}

/// Orchestrates current-repo `push` for existing commits only.
fn run_push_current(yes: bool, confirm: bool) -> ExitCode {
    use crate::commands::sync;

    let (client, target) = match plan_current_push() {
        Ok(planned) => planned,
        Err(error) => {
            eprintln!("push: {}", error_text(&error));
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

    let result = match client.execute_repository_push(v1::ExecuteRepositoryPushRequest {
        target: Some(sync::push_target_to_grpc(&target)),
        mode: v1::RepositoryPushMode::ExistingCommits as i32,
        message: None,
    }) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("push: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    finish_push_response(&result)
}

fn plan_current_push() -> anyhow::Result<(ServerClient, commands::sync::PushTarget)> {
    let repository_path = commands::canonical_working_directory()?
        .to_string_lossy()
        .into_owned();
    let client = ServerClient::connect()?;
    let response =
        client.plan_repository_push(v1::PlanRepositoryPushRequest { repository_path })?;
    let target = match response
        .outcome
        .ok_or_else(|| anyhow::anyhow!("gtl-server returned no push plan outcome"))?
    {
        v1::plan_repository_push_response::Outcome::Ready(target) => {
            commands::sync::push_target_from_grpc(target)?
        }
        v1::plan_repository_push_response::Outcome::Refused(refusal) => {
            anyhow::bail!(refusal.detail)
        }
    };
    Ok((client, target))
}

fn finish_push_response(response: &v1::ExecuteRepositoryPushResponse) -> ExitCode {
    match v1::RepositoryPushStatus::try_from(response.status) {
        Ok(v1::RepositoryPushStatus::NoOp | v1::RepositoryPushStatus::Completed) => {
            println!("push: {}", response.detail);
            ExitCode::Ok
        }
        Ok(v1::RepositoryPushStatus::Refused | v1::RepositoryPushStatus::Failed) => {
            eprintln!("push: {}", response.detail);
            ExitCode::Internal
        }
        Ok(v1::RepositoryPushStatus::Unspecified) | Err(_) => {
            eprintln!("push: gtl-server returned an invalid push status");
            ExitCode::Internal
        }
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

    let repo_path = match commands::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("commit: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let client = match ServerClient::connect() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("commit: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let response = match client.plan_repository_commit(v1::PlanRepositoryCommitRequest {
        repository_path: repo_path.to_string_lossy().into_owned(),
    }) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("commit: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let target = match response.outcome {
        Some(v1::plan_repository_commit_response::Outcome::Refused(refusal)) => {
            eprintln!("commit: {}", refusal.detail);
            return ExitCode::Internal;
        }
        Some(v1::plan_repository_commit_response::Outcome::Ready(target)) => {
            match sync::commit_target_from_grpc(target) {
                Ok(target) => target,
                Err(error) => {
                    eprintln!("commit: {}", error_text(&error));
                    return ExitCode::Internal;
                }
            }
        }
        None => {
            eprintln!("commit: gtl-server returned no commit plan outcome");
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

    let result = match client.execute_repository_commit(v1::ExecuteRepositoryCommitRequest {
        target: Some(sync::commit_target_to_grpc(&target)),
        message: message.into(),
    }) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("commit: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    match v1::RepositoryCommitStatus::try_from(result.status) {
        Ok(v1::RepositoryCommitStatus::Committed | v1::RepositoryCommitStatus::NoOp) => {
            println!("commit: {}", result.detail);
            ExitCode::Ok
        }
        Ok(v1::RepositoryCommitStatus::Failed) => {
            eprintln!("commit: {}", result.detail);
            ExitCode::Internal
        }
        Ok(v1::RepositoryCommitStatus::Unspecified) | Err(_) => {
            eprintln!("commit: gtl-server returned an invalid commit status");
            ExitCode::Internal
        }
    }
}

/// Orchestrates recursive `push -r`: discover every repo under the current directory, show the
/// confirmation listing each repo's push destination, gate on `--yes`/TTY like
/// `push "<message>"`, then
/// push. Discovery and the resolved destinations are read-only and local — no fetch. The
/// interactive prompt is the only side effect kept out of the
/// repository operations.
fn run_push_subrepos(yes: bool) -> ExitCode {
    use crate::commands::push_subrepos::{
        confirmation, result_from_grpc, targets_from_grpc, targets_to_grpc,
    };

    let root = match commands::canonical_working_directory() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("push -r: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };

    let client = match ServerClient::connect() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("push -r: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let plan = match client.plan_recursive_repository_push(v1::PlanRecursiveRepositoryPushRequest {
        root: root.to_string_lossy().into_owned(),
    }) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("push -r: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let targets = match plan.outcome {
        Some(v1::plan_recursive_repository_push_response::Outcome::Ready(plan)) => {
            match targets_from_grpc(plan.targets) {
                Ok(targets) => targets,
                Err(error) => {
                    eprintln!("push -r: {}", error_text(&error));
                    return ExitCode::Internal;
                }
            }
        }
        Some(v1::plan_recursive_repository_push_response::Outcome::Refused(refusal)) => {
            eprintln!("push -r: {}", refusal.detail);
            return ExitCode::Internal;
        }
        None => {
            eprintln!("push -r: gtl-server returned no recursive-push plan outcome");
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

    let result =
        match client.execute_recursive_repository_push(v1::ExecuteRecursiveRepositoryPushRequest {
            targets: targets_to_grpc(&targets),
        }) {
            Ok(response) => match result_from_grpc(response) {
                Ok(result) => result,
                Err(error) => {
                    eprintln!("push -r: {}", error_text(&error));
                    return ExitCode::Internal;
                }
            },
            Err(error) => {
                eprintln!("push -r: {}", error_text(&error));
                return ExitCode::Internal;
            }
        };
    let detail = format_push_subrepos_result(&result);
    match result.status {
        gtl_models::repository::recursive_push::Status::Ok => {
            println!("{detail}");
            ExitCode::Ok
        }
        gtl_models::repository::recursive_push::Status::Partial
        | gtl_models::repository::recursive_push::Status::Fail => {
            eprintln!("{detail}");
            ExitCode::Internal
        }
    }
}

fn format_push_subrepos_result(
    result: &gtl_models::repository::recursive_push::PushAllResult,
) -> String {
    use gtl_models::repository::recursive_push::{RepoOutcome, Status};

    let summary = PushSummary::from_outcomes(
        result.reports.iter().map(|report| match &report.outcome {
            RepoOutcome::Pushed => PushOutcome::Pushed,
            RepoOutcome::UpToDate | RepoOutcome::Skipped(_) => PushOutcome::Skipped,
            RepoOutcome::Failed(_) => PushOutcome::Failed,
        }),
        false,
        0,
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
    run_switch_with_path(args, &repo_path)
}

fn run_switch_with_path(args: &SwitchArgs, repo_path: &Path) -> ExitCode {
    let onto = match parse_branch_name("switch", args.onto.as_deref().unwrap_or("main")) {
        Ok(onto) => onto,
        Err(code) => return code,
    };
    let action = if args.revert {
        v1::RepositoryBranchAction::Revert
    } else if args.rebase {
        v1::RepositoryBranchAction::Rebase
    } else {
        v1::RepositoryBranchAction::Switch
    };
    let client = match ServerClient::connect() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("switch: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let result = match client.change_repository_branch(v1::ChangeRepositoryBranchRequest {
        repository_path: repo_path.to_string_lossy().into_owned(),
        onto_branch: onto.to_string(),
        action: action as i32,
    }) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("switch: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    match v1::RepositoryBranchStatus::try_from(result.status) {
        Ok(v1::RepositoryBranchStatus::FastForwarded) => {
            println!("{}", result.detail);
            if args.diff {
                diff_exit(commands::diff::run(
                    &DiffTarget::Unpushed { pinned: None },
                    None,
                    false,
                ))
            } else {
                ExitCode::Ok
            }
        }
        Ok(
            v1::RepositoryBranchStatus::Switched
            | v1::RepositoryBranchStatus::AlreadyThere
            | v1::RepositoryBranchStatus::Reverted
            | v1::RepositoryBranchStatus::NoOp,
        ) => {
            println!("{}", result.detail);
            ExitCode::Ok
        }
        Ok(v1::RepositoryBranchStatus::Refused | v1::RepositoryBranchStatus::Failed) => {
            eprintln!("switch: {}", result.detail);
            ExitCode::Internal
        }
        Ok(v1::RepositoryBranchStatus::Unspecified) | Err(_) => {
            eprintln!("switch: gtl-server returned an invalid branch-change status");
            ExitCode::Internal
        }
    }
}

fn parse_branch_name(command: &str, raw: &str) -> Result<BranchName, ExitCode> {
    BranchName::try_new(raw.to_owned()).map_err(|error| {
        eprintln!("{command}: invalid branch name: {error}");
        ExitCode::Usage
    })
}

/// Orchestrates `prune`: `--all` fans out over managed repos (preview unless `-y`); the
/// single-repo path plans read-only, shows the will-delete block, gates on `-y`/TTY like
/// `push "<message>"`, then deletes. All git work is local; refusals → stderr, logs → stdout.
fn run_prune(args: &PruneArgs) -> ExitCode {
    let onto = match parse_branch_name("prune", args.onto.as_deref().unwrap_or("main")) {
        Ok(onto) => onto,
        Err(code) => return code,
    };

    if args.all {
        let options = ManagedOptions {
            dry: !args.yes,
            output: ManagedOutput::from_flags(args.json, false),
            message_for_all: None,
            interactive: confirm::stdin_is_terminal(),
        };
        return managed_exit(&commands::managed::run_prune_all(&onto, &options));
    }

    let repo_path = match commands::canonical_working_directory() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("prune: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    run_prune_current(args, &onto, &repo_path)
}

fn run_prune_current(args: &PruneArgs, onto: &BranchName, repo_path: &Path) -> ExitCode {
    use crate::commands::prune;

    let client = match ServerClient::connect() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("prune: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let response = match client.plan_repository_prune(v1::PlanRepositoryPruneRequest {
        repository_path: repo_path.to_string_lossy().into_owned(),
        onto_branch: onto.to_string(),
    }) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("prune: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let plan = match response.outcome {
        Some(v1::plan_repository_prune_response::Outcome::Refused(refusal)) => {
            eprintln!("prune: {}", refusal.detail);
            return ExitCode::Internal;
        }
        Some(v1::plan_repository_prune_response::Outcome::Nothing(nothing)) => {
            println!("{}", nothing.detail);
            return ExitCode::Ok;
        }
        Some(v1::plan_repository_prune_response::Outcome::Ready(plan)) => plan,
        None => {
            eprintln!("prune: gtl-server returned no prune plan outcome");
            return ExitCode::Internal;
        }
    };
    let repository_root = plan.repository_root.clone();
    let branches = match prune::plan_from_grpc(plan) {
        Ok(branches) => branches,
        Err(error) => {
            eprintln!("prune: {}", error_text(&error));
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

    let response = match client.execute_repository_prune(v1::ExecuteRepositoryPruneRequest {
        plan: Some(prune::plan_to_grpc(repository_root, &branches)),
    }) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("prune: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let result = match prune::result_from_grpc(response) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("prune: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };
    let detail = prune::render_result(&result);
    match result.status {
        prune::PruneStatus::Ok => {
            println!("{detail}");
            ExitCode::Ok
        }
        prune::PruneStatus::Partial | prune::PruneStatus::Failed => {
            eprintln!("prune: {detail}");
            ExitCode::Internal
        }
        prune::PruneStatus::Aborted => {
            if !result.deleted.is_empty() || !result.failed.is_empty() {
                println!("{detail}");
            }
            eprintln!(
                "prune: {}",
                result
                    .failure_detail
                    .as_deref()
                    .unwrap_or("branch prune stopped before completion")
            );
            ExitCode::Internal
        }
    }
}

/// Dispatches `status` by scope: `--all` uses sample_project's active projects, `-r` recursively scans
/// the current directory, default ⇒ the current repo alone.
fn run_status(args: &StatusArgs) -> ManagedRun<commands::managed::StatusResult> {
    let options = managed_read_options(args.read);
    if args.all {
        commands::managed::run_status(&options)
    } else if args.recursive {
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
        stderr: format!("status: {}", error_text(error)),
    }
}

fn canonical_working_directory_or_exit(command: &str) -> Result<PathBuf, ExitCode> {
    commands::canonical_working_directory().map_err(|error| {
        eprintln!("{command}: {}", error_text(&error));
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
        dry: false,
        output: ManagedOutput::from_flags(args.json, color),
        message_for_all: None,
        interactive: false,
    }
}

/// Builds the [`ManagedOptions`] for a fan-out command from its parsed flags.
fn managed_options(args: ManagedArgs, message_for_all: Option<String>) -> ManagedOptions {
    ManagedOptions {
        dry: args.dry,
        output: ManagedOutput::from_flags(args.json, false),
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

pub(crate) fn error_text(error: &anyhow::Error) -> String {
    for source in error.chain() {
        if let Some(error) = source.downcast_ref::<gtl_client::ClientError>() {
            return error.status().message().to_owned();
        }
        if let Some(status) = source
            .downcast_ref::<gtl_client::ConnectError>()
            .and_then(gtl_client::ConnectError::status)
        {
            return status.message().to_owned();
        }
    }
    format!("{error:#}")
}

/// Map a [`commands::diff::DiffOutcome`] result to an [`ExitCode`]: either `Ok` variant
/// (an artifact was rendered, or a clean empty-range no-op) is a success. Shared by every
/// render path that produces a `DiffOutcome` — `diff`, `diff -r`, `diff --all`,
/// and `diff merge`.
fn diff_exit(result: anyhow::Result<commands::diff::DiffOutcome>) -> ExitCode {
    match result {
        Ok(_) => ExitCode::Ok,
        Err(error) => {
            eprintln!("{}", error_text(&error));
            ExitCode::Internal
        }
    }
}

/// Map a `diff live` result to an [`ExitCode`]: success (a save, or a clean
/// no-managed-repos-unpushed no-op) is `Ok`; a validation rejection or transport
/// failure prints the server's own message and exits `Internal`.
fn diff_live_exit(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::Ok,
        Err(error) => {
            eprintln!("{}", error_text(&error));
            ExitCode::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target_args(target: Option<&str>) -> DiffTargetArgs {
        DiffTargetArgs {
            scope: crate::cli::DiffScopeArgs {
                all: false,
                recursive: false,
                worktrees: false,
            },
            unpushed: false,
            target: target.map(str::to_owned),
            last: None,
            merge: None,
            name: None,
            set_theme: None,
        }
    }

    #[test]
    fn diff_target_maps_absent_and_blank_positionals_to_unpushed() {
        for target in [None, Some(""), Some("   ")] {
            assert_eq!(
                diff_target(target_args(target)).expect("target is valid"),
                DiffTarget::Unpushed { pinned: None }
            );
        }
    }

    #[test]
    fn diff_target_distinguishes_exact_ranges_from_base_revisions() {
        assert_eq!(
            diff_target(target_args(Some("abc123..def456"))).expect("range is valid"),
            DiffTarget::Range {
                range: gtl_models::git::GitRange::try_new("abc123..def456")
                    .expect("fixture range is non-empty"),
                pinned: None,
            }
        );
        assert_eq!(
            diff_target(target_args(Some("abc123"))).expect("revision is valid"),
            DiffTarget::Base(
                gtl_models::git::GitRevision::try_new("abc123")
                    .expect("fixture revision is non-empty")
            )
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
    fn prune_help_exits_ok() {
        assert_eq!(run(&["prune".into(), "--help".into()]), ExitCode::Ok);
    }

    #[test]
    fn unknown_command_is_usage() {
        assert_eq!(run(&["bogus".into()]), ExitCode::Usage);
    }

    #[test]
    fn diff_errors_print_without_cli_prefix() {
        let error = anyhow::anyhow!("fatal: bad ref\nnot a commit: nope");

        assert_eq!(error_text(&error), "fatal: bad ref\nnot a commit: nope");
    }
}
