use gtl_models::projects::catalogue::ProjectId;
use gtl_wire::v1;

use crate::{
    cli::{
        Cli, ColorChoice, Command, DiffArgs, DiffSub, DiffTarget, DiffTargetArgs,
        DiffTargetParseError, ManagedArgs, ManagedReadArgs, MergeArgs, ProjectCommand, PullArgs,
        PushArgs, ServerArgs, ServerCommand, StatusArgs, Theme,
    },
    commands::managed::{
        ManagedExit, ManagedOptions, ManagedOutput, ManagedRun, PushOutcome, PushSummary,
    },
    server_client::ServerClient,
};

pub mod cli;
pub mod commands;
pub mod preprocess;

mod confirm;
mod diff_viewer_client;
mod output;
mod server_client;
#[cfg(test)]
mod testing;
mod viewer;

/// Exit codes 0, 1, and 2 are stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Ok = 0,
    Internal = 1,
    Usage = 2,
}

#[must_use]
pub fn run(args: &[String]) -> ExitCode {
    let args = preprocess::normalize(args.to_vec());
    match Cli::parse_args(&args) {
        Ok(cli) => dispatch(cli.command),
        Err(error) => render_clap_error(&error),
    }
}

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
        Command::Push(args) => run_push(args),
        Command::Pull(args) => run_pull(&args),
        Command::Project(args) => run_project(args.command),
        Command::Tag(args) => commands::tag::run(args.command, args.commits, args.state),
        Command::Status(args) => managed_exit(&run_status(&args)),
        Command::Ls(args) => run_project(ProjectCommand::Ls(args)),
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
                return run_recursive_diff(&args.target, raw, args.repository.id.as_ref());
            }
            let name = args.target.name.clone();
            match diff_target(args.target) {
                Ok(target) => diff_exit(
                    commands::repository_path(args.repository.id.as_ref())
                        .and_then(|root| commands::diff::run(&root, &target, name.as_deref(), raw)),
                ),
                Err(error) => {
                    eprintln!("diff: {error}");
                    ExitCode::Usage
                }
            }
        }
    }
}

fn run_recursive_diff(target: &DiffTargetArgs, raw: bool, id: Option<&ProjectId>) -> ExitCode {
    diff_exit(commands::repository_path(id).and_then(|root| {
        commands::diff_subrepos::run_scan(root, target.last, target.scope.worktrees, raw)
    }))
}

fn run_project(command: ProjectCommand) -> ExitCode {
    match command {
        ProjectCommand::Ls(args) => managed_exit(&commands::managed::run_status(
            &managed_read_options(args.read),
        )),
        ProjectCommand::Push(args) => managed_exit(&commands::managed::run_push_all(
            &managed_options(args.managed),
        )),
        ProjectCommand::Pull(args) => managed_exit(&commands::managed::run_pull_all(
            &managed_options(args.managed),
        )),
        ProjectCommand::Diff(args) => diff_exit(
            commands::canonical_working_directory()
                .and_then(|root| commands::diff_subrepos::run_managed_all(root, args.raw)),
        ),
    }
}

fn run_push(args: PushArgs) -> ExitCode {
    let id = args.repository.id.as_ref();
    if args.recursive {
        return run_push_subrepos(args.yes, id);
    }
    if let Some(message) = args.message {
        return run_push_with_message(&message, args.yes, id);
    }
    match ServerClient::connect().and_then(|client| client.get_push_confirmation_requirement()) {
        Ok(settings) => run_push_current(args.yes, settings.push_confirmation_required, id),
        Err(error) => {
            eprintln!("push: {}", error_text(&error));
            ExitCode::Internal
        }
    }
}

fn run_pull(args: &PullArgs) -> ExitCode {
    let result = commands::repository_path(args.repository.id.as_ref())
        .and_then(|root| commands::pull::run(&root, args.managed));
    match result {
        Ok(()) => ExitCode::Ok,
        Err(error) => {
            eprintln!("pull: {}", error_text(&error));
            ExitCode::Internal
        }
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

fn diff_target(args: DiffTargetArgs) -> Result<DiffTarget, DiffTargetParseError> {
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

fn run_push_with_message(message: &str, yes: bool, id: Option<&ProjectId>) -> ExitCode {
    use crate::commands::sync;

    if message.trim().is_empty() {
        eprintln!("push: a non-empty commit message is required");
        return ExitCode::Usage;
    }

    let (client, target) = match plan_push(id) {
        Ok(planned) => planned,
        Err(error) => {
            eprintln!("push: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };

    if target.pending.changed.is_zero() && target.pending.ahead.into_inner() == 0 {
        println!("Nothing to commit or push");
        return ExitCode::Ok;
    }
    if let Err(exit) = confirm::request("push", yes, &sync::confirmation(&target)) {
        return exit;
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

fn run_push_current(yes: bool, confirm: bool, id: Option<&ProjectId>) -> ExitCode {
    use crate::commands::sync;

    let (client, target) = match plan_push(id) {
        Ok(planned) => planned,
        Err(error) => {
            eprintln!("push: {}", error_text(&error));
            return ExitCode::Internal;
        }
    };

    if !target.pending.changed.is_zero() {
        eprintln!("push: working tree has uncommitted changes");
        return ExitCode::Internal;
    }
    if target.pending.ahead.into_inner() == 0 {
        println!("Already up to date");
        return ExitCode::Ok;
    }
    if let Err(exit) = confirm::request("push", yes || !confirm, &sync::push_confirmation(&target))
    {
        return exit;
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

fn plan_push(id: Option<&ProjectId>) -> anyhow::Result<(ServerClient, commands::sync::PushTarget)> {
    let repository_path = commands::repository_path(id)?
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
            println!("{}", output::sentence(&response.detail));
            ExitCode::Ok
        }
        Ok(v1::RepositoryPushStatus::Refused | v1::RepositoryPushStatus::Failed) => {
            eprintln!("push: {}", response.detail);
            commands::sync::print_failure_progress(response.progress.as_ref());
            ExitCode::Internal
        }
        Ok(v1::RepositoryPushStatus::Unspecified) | Err(_) => {
            eprintln!("push: gtl-server returned an invalid push status");
            ExitCode::Internal
        }
    }
}

fn run_push_subrepos(yes: bool, id: Option<&ProjectId>) -> ExitCode {
    use crate::commands::push_subrepos::{
        confirmation, result_from_grpc, targets_from_grpc, targets_to_grpc,
    };

    let root = match commands::repository_path(id) {
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

    if !targets.iter().any(|target| {
        matches!(
            target.dest,
            gtl_models::repository::recursive_push::Dest::Push { .. }
        )
    }) {
        let reports = targets
            .iter()
            .map(|target| {
                use gtl_models::repository::recursive_push::{Dest, RepoOutcome, RepoReport};
                let outcome = match &target.dest {
                    Dest::Skip { reason } => RepoOutcome::Skipped(reason.clone()),
                    Dest::Push { .. } | Dest::Synced { .. } => RepoOutcome::UpToDate,
                };
                RepoReport {
                    label: target.label.clone(),
                    outcome,
                }
            })
            .collect();
        println!(
            "{}",
            format_push_subrepos_result(&gtl_models::repository::recursive_push::PushAllResult {
                status: gtl_models::repository::recursive_push::Status::Ok,
                reports,
            })
        );
        return ExitCode::Ok;
    }
    if let Err(exit) = confirm::request("push -r", yes, &confirmation(&targets)) {
        return exit;
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
    println!("{detail}");
    match result.status {
        gtl_models::repository::recursive_push::Status::Ok => ExitCode::Ok,
        gtl_models::repository::recursive_push::Status::Partial
        | gtl_models::repository::recursive_push::Status::Fail => ExitCode::Internal,
    }
}

fn format_push_subrepos_result(
    result: &gtl_models::repository::recursive_push::PushAllResult,
) -> String {
    use gtl_models::repository::recursive_push::RepoOutcome;

    let summary = PushSummary::from_outcomes(
        result.reports.iter().map(|report| match &report.outcome {
            RepoOutcome::Pushed => PushOutcome::Pushed,
            RepoOutcome::UpToDate => PushOutcome::UpToDate,
            RepoOutcome::Skipped(_) => PushOutcome::Skipped,
            RepoOutcome::Failed(_) => PushOutcome::Failed,
        }),
        false,
        0,
    );
    let rows = result
        .reports
        .iter()
        .map(|report| {
            let (status, detail) = match &report.outcome {
                RepoOutcome::Pushed => ("Pushed", ""),
                RepoOutcome::UpToDate => ("Up to date", ""),
                RepoOutcome::Skipped(reason) => ("Skipped", reason.as_str()),
                RepoOutcome::Failed(reason) => ("Failed", reason.as_str()),
            };
            [
                report.label.to_string(),
                status.to_string(),
                detail.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    format!(
        "{}\n\n{}",
        output::table(
            ["Project", "Result", "Detail"],
            &rows,
            output::stdout_color()
        ),
        summary.render()
    )
}

fn run_status(args: &StatusArgs) -> ManagedRun<commands::managed::StatusResult> {
    let options = managed_read_options(args.read);
    if args.recursive {
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

fn managed_read_options(args: ManagedReadArgs) -> ManagedOptions {
    let color = match args.color {
        ColorChoice::Auto => output::stdout_color(),
        ColorChoice::Always => true,
        ColorChoice::Never => false,
    };

    ManagedOptions {
        dry: false,
        output: ManagedOutput::from_flags(args.json, color),
    }
}

fn managed_options(args: ManagedArgs) -> ManagedOptions {
    ManagedOptions {
        dry: args.dry,
        output: ManagedOutput::from_flags(args.json, output::stdout_color()),
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
        ManagedExit::Fail => ExitCode::Usage,
    }
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

fn diff_exit(result: anyhow::Result<commands::diff::DiffOutcome>) -> ExitCode {
    match result {
        Ok(_) => ExitCode::Ok,
        Err(error) => {
            eprintln!("{}", error_text(&error));
            ExitCode::Internal
        }
    }
}

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
                diff_target(target_args(target)).unwrap(),
                DiffTarget::Unpushed { pinned: None }
            );
        }
    }

    #[test]
    fn diff_target_distinguishes_exact_ranges_from_base_revisions() {
        assert_eq!(
            diff_target(target_args(Some("abc123..def456"))).unwrap(),
            DiffTarget::Range {
                range: gtl_models::git::GitRange::try_new("abc123..def456").unwrap(),
                pinned: None,
            }
        );
        assert_eq!(
            diff_target(target_args(Some("abc123"))).unwrap(),
            DiffTarget::Base(gtl_models::git::GitRevision::try_new("abc123").unwrap())
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
    fn unknown_command_is_usage() {
        assert_eq!(run(&["bogus".into()]), ExitCode::Usage);
    }

    #[test]
    fn diff_errors_print_without_cli_prefix() {
        let error = anyhow::anyhow!("fatal: bad ref\nnot a commit: nope");

        assert_eq!(error_text(&error), "fatal: bad ref\nnot a commit: nope");
    }
}
