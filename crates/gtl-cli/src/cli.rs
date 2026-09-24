use std::num::NonZeroU32;

use clap::{Args, Parser, Subcommand, ValueEnum};
use gtl_models::{
    diffs::PinnedRange,
    git::{GitRange, GitRevision},
    projects::catalogue::ProjectId,
};

fn non_empty_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty() {
        Err("name must not be blank".to_string())
    } else {
        Ok(name.to_string())
    }
}

fn non_empty_message(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("message must not be blank".to_string())
    } else {
        Ok(value.to_string())
    }
}

fn project_id(value: &str) -> Result<ProjectId, String> {
    ProjectId::try_new(value.to_ascii_uppercase())
        .map_err(|_| "project ID must contain 2 to 4 ASCII letters".to_string())
}

/// Documents the stable process exit codes that `crate::ExitCode` defines.
const EXIT_STATUS_HELP: &str = "\
Exit status:
  0  Success, or a declined confirmation
  1  The operation failed
  2  Invalid arguments or input
  3  The repository, project, or settings state refused the operation
  4  gtl-server is unreachable, busy, or timed out; retrying may succeed";

/// Inspect repositories and run Git workflows.
#[derive(Debug, Parser)]
#[command(
    name = "git-tools",
    version,
    about,
    long_about = None,
    after_long_help = EXIT_STATUS_HELP,
    arg_required_else_help = true,
    styles = clap_cargo::style::CLAP_STYLING
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Parses arguments without `argv[0]`.
    pub fn parse_args(args: &[String]) -> Result<Self, clap::Error> {
        let full_argv = std::iter::once(String::from("git-tools")).chain(args.iter().cloned());
        Self::try_parse_from(full_argv)
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// View a repository diff.
    #[command(visible_alias = "d")]
    Diff(DiffArgs),
    /// Push commits, optionally committing all changes first.
    #[command(visible_alias = "p")]
    Push(PushArgs),
    /// Fast-forward a repository from origin.
    Pull(PullArgs),
    /// Run workflows across managed projects.
    Project(ProjectArgs),
    /// List tags, show tag commits, or push tags.
    Tag(TagArgs),
    /// Show repository status.
    #[command(visible_alias = "s")]
    Status(StatusArgs),
    /// Alias for `project ls`.
    #[command(hide = true)]
    Ls(LsArgs),
    /// Inspect the resident gtl-server.
    Server(ServerArgs),
}

#[derive(Debug, Args)]
pub struct ServerArgs {
    #[command(subcommand)]
    pub command: ServerCommand,
}

#[derive(Debug, Subcommand)]
pub enum ServerCommand {
    /// Check the private local gRPC health endpoint.
    Status,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct DiffArgs {
    #[command(flatten)]
    pub repository: RepositoryArgs,
    #[command(subcommand)]
    pub sub: Option<DiffSub>,
    /// Render an artifact and print its URL without opening a viewer.
    #[arg(long)]
    pub raw: bool,
    #[command(flatten)]
    pub target: DiffTargetArgs,
}

#[derive(Debug, Subcommand)]
pub enum DiffSub {
    /// Render a merge diff artifact (three-dot diff) against a base branch.
    Merge(MergeArgs),
    /// Save and open a live upstream or local-branch comparison.
    Live(LiveArgs),
}

#[derive(Debug, Args)]
pub struct MergeArgs {
    /// Subrepo working tree to render.
    #[arg(long = "repo")]
    pub repo_path: String,
    /// Base branch to merge into (default: main).
    #[arg(long)]
    pub base: Option<String>,
    /// Render an artifact and print its URL without opening a viewer.
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Args)]
pub struct LiveArgs {
    /// Repo to save + open a live view for (default: every managed repo with
    /// commits ahead of its upstream or local comparison branch).
    #[arg(long)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct RepositoryArgs {
    /// Select a managed project by ID instead of the current repository.
    #[arg(long, value_name = "PROJECT_ID", value_parser = project_id)]
    pub id: Option<ProjectId>,
}

#[derive(Debug, Args)]
pub struct PushArgs {
    #[command(flatten)]
    pub repository: RepositoryArgs,
    /// Stage all changes, commit with MESSAGE, and push.
    #[arg(value_parser = non_empty_message, conflicts_with = "recursive")]
    pub message: Option<String>,
    /// Push the repository and its nested subrepos.
    #[arg(short = 'r', long)]
    pub recursive: bool,
    /// Skip confirmation.
    #[arg(short = 'y', long)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct PullArgs {
    #[command(flatten)]
    pub repository: RepositoryArgs,
    #[command(flatten)]
    pub managed: ManagedArgs,
}

#[derive(Debug, Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub command: ProjectCommand,
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// List active projects and their repository status.
    Ls(LsArgs),
    /// Push active projects, respecting configured push exclusions.
    Push(ProjectPushArgs),
    /// Fast-forward all active projects from origin.
    Pull(ProjectPullArgs),
    /// View diffs for active projects with unpushed changes.
    Diff(ProjectDiffArgs),
}

#[derive(Debug, Args)]
pub struct ProjectPushArgs {
    /// Operate on all active projects except configured push exclusions.
    #[arg(long, required = true)]
    pub all: bool,
    #[command(flatten)]
    pub managed: ManagedArgs,
}

#[derive(Debug, Args)]
pub struct ProjectPullArgs {
    /// Operate on all active projects.
    #[arg(long, required = true)]
    pub all: bool,
    #[command(flatten)]
    pub managed: ManagedArgs,
}

#[derive(Debug, Args)]
pub struct ProjectDiffArgs {
    /// Operate on all active projects.
    #[arg(long, required = true)]
    pub all: bool,
    /// Print an artifact URL without opening a viewer.
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Args)]
pub struct TagArgs {
    #[command(subcommand)]
    pub command: Option<TagCommand>,
    /// Show the commit each tag points at.
    #[arg(short = 'c', long = "commits")]
    pub commits: bool,
    /// Query origin and mark each tag [local] or [remote].
    #[arg(short = 's', long = "state")]
    pub state: bool,
}

#[derive(Debug, Subcommand)]
pub enum TagCommand {
    /// List local tags and each annotated tag's message (first line).
    Ls,
    /// Create an annotated tag.
    Add {
        /// Tag name to create.
        tag: String,
        /// Annotated tag message.
        message: String,
    },
    /// Push local tags, or create one annotated tag and push it.
    #[command(visible_alias = "p")]
    Push {
        /// Optional tag name to create (or, with `--label`, the existing tag to label) before
        /// pushing.
        tag: Option<String>,
        /// Annotated tag message when creating a tag.
        message: Option<String>,
        /// Lightweight label tag to point at the same commit as `tag`.
        #[arg(short = 'l', long = "label")]
        label: Option<String>,
    },
    /// Preview and create the next tag from the project's configured tag pattern.
    #[command(visible_alias = "b")]
    Bump {
        /// Annotated tag message.
        #[arg(allow_hyphen_values = true, value_parser = non_empty_message)]
        message: String,
        /// `major`, `minor`, or `patch` or index of rightmost slot. Defaults to the rightmost
        /// slot.
        #[arg(short = 'n', long = "level", value_parser = parse_tag_bump_level, default_value = "0")]
        level: TagBumpLevel,
        /// Configured tag pattern name.
        #[arg(long, alias = "pt")]
        pattern: Option<String>,
        /// Push only the newly created tag to origin.
        #[arg(short = 'p', long)]
        push: bool,
        /// Show the exact proposed mutation without creating or pushing a tag.
        #[arg(long, conflicts_with = "yes")]
        dry: bool,
        /// Create the tag without prompting or printing a review.
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagBumpLevel {
    Slot(u32),
    Major,
    Minor,
    Patch,
}

fn parse_tag_bump_level(value: &str) -> Result<TagBumpLevel, String> {
    match value {
        "major" => Ok(TagBumpLevel::Major),
        "minor" => Ok(TagBumpLevel::Minor),
        "patch" => Ok(TagBumpLevel::Patch),
        index => index.parse::<u32>().map(TagBumpLevel::Slot).map_err(|_| {
            "level must be a slot index counted from the right, or one of major, minor, patch"
                .to_string()
        }),
    }
}

#[derive(Debug, Args)]
pub struct DiffTargetArgs {
    #[command(flatten)]
    pub scope: DiffScopeArgs,
    /// Diff unpushed commits, or committed branch changes against the project comparison
    /// branch (default: local main) when no upstream exists. This is the default target.
    #[arg(long, conflicts_with_all = ["target", "last", "recursive"])]
    pub unpushed: bool,
    /// Base commit (including staged, unstaged, and untracked changes), a
    /// `<start>..<end>` committed range, or omitted for unpushed work.
    #[arg(conflicts_with_all = ["last", "recursive"])]
    pub target: Option<String>,
    /// Diff the last N commits (`HEAD~N..HEAD`); bare `-l` diffs the last commit.
    #[arg(short = 'l', long = "last", value_name = "N", num_args = 0..=1, default_missing_value = "1")]
    pub last: Option<NonZeroU32>,
    /// Diff what merging HEAD into BASE would introduce (`BASE...HEAD`).
    #[arg(short = 'm', long = "merge", value_name = "BASE", conflicts_with_all = ["target", "last", "unpushed", "recursive"])]
    pub merge: Option<String>,
    /// Name the generated diff in the viewer history label.
    ///
    /// Only valid for the single-repo diff modes.
    #[arg(short = 'n', long = "name", value_name = "NAME", value_parser = non_empty_name)]
    pub name: Option<String>,
    /// Persist the diff-artifact theme to the user config and exit without rendering.
    /// The same `theme` key stays editable by hand in the config TOML.
    #[arg(
        long,
        value_name = "THEME",
        conflicts_with_all = ["id", "unpushed", "target", "last", "recursive", "worktrees", "merge", "name"],
    )]
    pub set_theme: Option<Theme>,
}

#[derive(Debug, Args)]
pub struct DiffScopeArgs {
    /// Render one tabbed HTML diff for every git repo under the current directory.
    #[arg(short = 'r', long = "recursive", conflicts_with_all = ["target", "merge", "name"])]
    pub recursive: bool,
    /// Include nested linked worktrees in a recursive diff scan.
    #[arg(short = 'w', long = "worktrees", requires = "recursive")]
    pub worktrees: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum Theme {
    Dark,
    Mirage,
    Glacier,
    Graphite,
    Carbon,
}

impl From<Theme> for gtl_models::viewer::Theme {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::Dark,
            Theme::Mirage => Self::Mirage,
            Theme::Glacier => Self::Glacier,
            Theme::Graphite => Self::Graphite,
            Theme::Carbon => Self::Carbon,
        }
    }
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Report the current repo plus any nested subrepos under the current directory (nested
    /// linked worktrees are skipped).
    #[arg(short = 'r', long)]
    pub recursive: bool,
    #[command(flatten)]
    pub read: ManagedReadArgs,
}

#[derive(Debug, Args)]
pub struct LsArgs {
    #[command(flatten)]
    pub read: ManagedReadArgs,
}

#[derive(Debug, Clone, Copy, Args)]
pub struct ManagedReadArgs {
    /// Serializes output as JSON
    #[arg(long)]
    pub json: bool,
    /// When to emit ANSI colors in human output.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    /// Color only when stdout is a terminal.
    Auto,
    /// Always emit ANSI color codes.
    Always,
    /// Never emit ANSI color codes.
    Never,
}

#[derive(Debug, Clone, Copy, Args)]
pub struct ManagedArgs {
    /// Preview changes without pushing or merging.
    #[arg(long)]
    pub dry: bool,
    /// Serializes output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffTarget {
    Unpushed {
        pinned: Option<PinnedRange>,
    },
    Base(GitRevision),
    Range {
        range: GitRange,
        pinned: Option<PinnedRange>,
    },
    Merge {
        base: GitRevision,
        pinned: Option<PinnedRange>,
    },
    Last {
        count: NonZeroU32,
        pinned: Option<PinnedRange>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiffTargetParseError {
    #[error("Git revision must not be empty")]
    EmptyRevision,
    #[error("Git range must not be empty")]
    EmptyRange,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory as _;

    use super::*;

    #[test]
    fn long_help_documents_every_exit_status() {
        let help = Cli::command().render_long_help().to_string();

        for (code, meaning) in [
            ("0", "Success"),
            ("1", "failed"),
            ("2", "Invalid arguments"),
            ("3", "refused"),
            ("4", "unreachable"),
        ] {
            assert!(
                help.lines()
                    .any(|line| line.trim_start().starts_with(code) && line.contains(meaning)),
                "missing exit status {code}"
            );
        }
    }

    #[test]
    fn parse_args_tag_bump_accepts_short_aliases() {
        let cli = Cli::parse_args(&[
            "tag".into(),
            "b".into(),
            "release".into(),
            "-n".into(),
            "patch".into(),
            "--pt".into(),
            "release".into(),
            "-p".into(),
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Tag(TagArgs {
                command: Some(TagCommand::Bump {
                    level: TagBumpLevel::Patch,
                    message,
                    pattern: Some(pattern),
                    push: true,
                    dry: false,
                    yes: false,
                }),
                ..
            }) if message == "release" && pattern == "release"
        ));
    }

    #[test]
    fn parse_args_tag_bump_defaults_to_the_rightmost_slot_and_accepts_indexes() {
        let cli = Cli::parse_args(&["tag".into(), "bump".into(), "patch".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Tag(TagArgs {
                command: Some(TagCommand::Bump {
                    level: TagBumpLevel::Slot(0),
                    message,
                    pattern: None,
                    ..
                }),
                ..
            }) if message == "patch"
        ));

        let cli = Cli::parse_args(&[
            "tag".into(),
            "bump".into(),
            "-n".into(),
            "2".into(),
            "msg".into(),
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Tag(TagArgs {
                command: Some(TagCommand::Bump {
                    level: TagBumpLevel::Slot(2),
                    ..
                }),
                ..
            })
        ));
        assert!(
            Cli::parse_args(&[
                "tag".into(),
                "bump".into(),
                "-n".into(),
                "pre".into(),
                "msg".into()
            ])
            .is_err()
        );
    }

    #[test]
    fn parse_args_diff_name_trims_and_sets_history_label() {
        let cli = Cli::parse_args(&["diff".into(), "-n".into(), "  eod  ".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Diff(DiffArgs {
                target: DiffTargetArgs {
                    name: Some(name),
                    ..
                },
                ..
            }) if name == "eod"
        ));
    }

    #[test]
    fn parse_args_diff_set_theme_conflicts_with_a_target() {
        assert!(
            Cli::parse_args(&[
                "diff".into(),
                "abc123".into(),
                "--set-theme".into(),
                "dark".into()
            ])
            .is_err()
        );
    }

    #[test]
    fn parse_args_diff_rejects_last_combined_with_target() {
        assert!(
            Cli::parse_args(&["diff".into(), "abc123".into(), "-l".into(), "2".into()]).is_err()
        );
    }

    #[test]
    fn parse_args_diff_rejects_merge_combined_with_other_targets() {
        assert!(
            Cli::parse_args(&["diff".into(), "-m".into(), "main".into(), "abc123".into()]).is_err()
        );
        assert!(
            Cli::parse_args(&["diff".into(), "-m".into(), "main".into(), "-l".into()]).is_err()
        );
    }

    #[test]
    fn parse_args_push_recursive_rejects_message() {
        assert!(Cli::parse_args(&["push".into(), "-r".into(), "save work".into()]).is_err());
    }
}
