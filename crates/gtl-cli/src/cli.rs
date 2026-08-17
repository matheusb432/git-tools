//! CLI surface, parsed with clap-derive. clap owns argument parsing, `--help`, and
//! `--version`. Each variant/field doc comment is the single source of truth for its help text.

use std::num::NonZeroU32;

use clap::{Args, Parser, Subcommand, ValueEnum};

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

/// git-tools — render Git workflow diff artifacts.
#[derive(Debug, Parser)]
#[command(
    name = "git-tools",
    version,
    about,
    long_about = None,
    arg_required_else_help = true,
    styles = clap_cargo::style::CLAP_STYLING
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Parses argv (already stripped of the program name).
    ///
    /// # Errors
    ///
    /// Returns the [`clap::Error`] for a usage problem, or the help/version request that
    /// clap models as an error (the caller maps that to a success exit).
    pub fn parse_args(args: &[String]) -> Result<Self, clap::Error> {
        let full_argv = std::iter::once(String::from("git-tools")).chain(args.iter().cloned());
        Self::try_parse_from(full_argv)
    }
}

/// Top-level subcommands; each maps to one [`crate::commands`] entry point.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Diff the current repo (all managed repos with `--all`, nested subrepos with `-r`),
    /// opening it in the app's viewer unless `--raw` prints an artifact URL instead.
    #[command(visible_alias = "d")]
    Diff(DiffArgs),
    /// Push existing commits, or stage all changes, commit with MESSAGE, and push.
    #[command(visible_alias = "p")]
    Push(PushArgs),
    /// Pull every active project listed by sample_project.
    Pull(PullArgs),
    /// Stage all changes and commit them, without pushing.
    Commit(CommitArgs),
    /// List tags, show tag commits, or push tags.
    Tag(TagArgs),
    /// Inspect git worktrees.
    #[command(visible_alias = "wk")]
    Worktree(WorktreeArgs),
    /// Switch to the main branch; with `--rebase`, fast-forward it onto the current branch's
    /// commits.
    #[command(visible_alias = "sw")]
    Switch(SwitchArgs),
    /// Show git status for the current repo; `--all` fans out over managed repos, `-r` recurses
    /// into nested subrepos.
    #[command(visible_alias = "s")]
    Status(StatusArgs),
    /// Aliases `status --all`
    Ls(LsArgs),
    /// Delete local branches whose commits are already merged into main.
    Prune(PruneArgs),
    /// Control the resident gtl-daemon.
    Daemon(DaemonArgs),
}

/// Arguments for `daemon`.
#[derive(Debug, Args)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub command: DaemonCommand,
}

/// Nested commands under `daemon`.
#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Report whether the daemon is running (port, pid, version).
    Status,
    /// Stop any healthy daemon and start a fresh process.
    Restart,
    /// Ask the daemon to exit.
    Stop,
}

/// Arguments for the root `diff` command and its nested subcommands.
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct DiffArgs {
    #[command(subcommand)]
    pub sub: Option<DiffSub>,
    /// Render an artifact and print its URL without opening a viewer.
    #[arg(long)]
    pub raw: bool,
    #[command(flatten)]
    pub target: DiffTargetArgs,
}

/// Nested commands under `diff`.
#[derive(Debug, Subcommand)]
pub enum DiffSub {
    /// Render a merge diff artifact (three-dot diff) against a base branch.
    Merge(MergeArgs),
    /// Save + open a persisted live view of unpushed work in a managed repo.
    Live(LiveArgs),
}

/// Arguments for `diff merge`.
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

/// Arguments for `diff live`. No `--raw`: a live view only ever renders through
/// the app because it has no store-artifact path.
#[derive(Debug, Args)]
pub struct LiveArgs {
    /// Repo to save + open a live view for (default: every managed repo with
    /// unpushed commits).
    #[arg(long)]
    pub path: Option<String>,
}

/// Arguments for `push`.
#[derive(Debug, Args)]
pub struct PushArgs {
    /// Commit message. When present, changes are staged and committed before pushing.
    #[arg(allow_hyphen_values = true, conflicts_with = "recursive")]
    pub message: Option<String>,
    /// Operate on every active project listed by sample_project.
    #[arg(long, conflicts_with = "recursive")]
    pub all: bool,
    /// Operate on the current repo plus nested subrepos under the current directory.
    #[arg(short = 'r', long, conflicts_with = "all")]
    pub recursive: bool,
    #[command(flatten)]
    pub managed: ManagedArgs,
    /// Skip confirmation where the selected push mode supports it.
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

/// Arguments for `commit`.
#[derive(Debug, Args)]
pub struct CommitArgs {
    /// Commit message.
    #[arg(allow_hyphen_values = true, required_unless_present = "all")]
    pub message: Option<String>,
    /// Operate on every active project listed by sample_project.
    #[arg(long)]
    pub all: bool,
    #[command(flatten)]
    pub managed: ManagedArgs,
    /// Skip confirmation where the selected commit mode supports it.
    #[arg(short = 'y', long = "yes", conflicts_with = "all")]
    pub yes: bool,
}

/// Arguments for `pull`.
#[derive(Debug, Args)]
pub struct PullArgs {
    /// Pull every active project listed by sample_project.
    #[arg(long, required = true)]
    pub all: bool,
    #[command(flatten)]
    pub managed: ManagedArgs,
}

/// Arguments for `tag`.
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

/// Nested commands under `tag`.
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
    /// Preview and create the next canonical `SemVer` tag.
    #[command(visible_alias = "b")]
    Bump {
        /// `SemVer` component to advance.
        level: TagBumpLevel,
        /// Annotated tag message.
        #[arg(allow_hyphen_values = true, value_parser = non_empty_message)]
        message: String,
        /// Push only the newly created tag to origin.
        #[arg(short = 'p', long)]
        push: bool,
        /// Show the exact proposed mutation without creating or pushing a tag.
        #[arg(long, conflicts_with = "yes")]
        dry: bool,
        /// Commit the displayed preview without prompting.
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },
}

/// `SemVer` component accepted by `tag bump`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum TagBumpLevel {
    Major,
    Minor,
    Patch,
}

/// Arguments for `worktree`.
#[derive(Debug, Args)]
pub struct WorktreeArgs {
    #[command(subcommand)]
    pub command: WorktreeCommand,
}

/// Nested commands under `worktree`.
#[derive(Debug, Subcommand)]
pub enum WorktreeCommand {
    /// Print the primary worktree path.
    Base,
    /// List worktrees in a readable table.
    Ls,
}

/// Target flags for the root `diff` command.
#[derive(Debug, Args)]
pub struct DiffTargetArgs {
    #[command(flatten)]
    pub scope: DiffScopeArgs,
    /// Diff unpushed work (`@{u}..HEAD`); this is also the default when no target is supplied.
    #[arg(long, conflicts_with_all = ["target", "last", "recursive"])]
    pub unpushed: bool,
    /// Base commit, a `<start>..<end>` range, or empty/omitted for unpushed work.
    #[arg(conflicts_with_all = ["last", "recursive"])]
    pub target: Option<String>,
    /// Diff the last N commits (`HEAD~N..HEAD`); bare `-l` diffs the last commit.
    #[arg(short = 'l', long = "last", value_name = "N", num_args = 0..=1, default_missing_value = "1")]
    pub last: Option<NonZeroU32>,
    /// Diff what merging HEAD into BASE would introduce (`BASE...HEAD`).
    #[arg(short = 'm', long = "merge", value_name = "BASE", conflicts_with_all = ["target", "last", "unpushed", "all", "recursive"])]
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
        conflicts_with_all = ["all", "unpushed", "target", "last", "recursive", "worktrees", "merge", "name"],
    )]
    pub set_theme: Option<Theme>,
}

/// Repository selection flags for the root `diff` command.
#[derive(Debug, Args)]
pub struct DiffScopeArgs {
    /// Render one tabbed HTML diff for all managed repos with unpushed commits.
    #[arg(long, conflicts_with_all = ["target", "last", "unpushed", "recursive"])]
    pub all: bool,
    /// Render one tabbed HTML diff for every git repo under the current directory.
    #[arg(short = 'r', long = "recursive", conflicts_with_all = ["all", "target", "merge", "name"])]
    pub recursive: bool,
    /// Include nested linked worktrees in a recursive diff scan.
    #[arg(short = 'w', long = "worktrees", requires = "recursive")]
    pub worktrees: bool,
}

/// Diff artifact color theme persisted to the user config by `diff --set-theme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum Theme {
    Dark,
    Light,
    Hearth,
    Mirage,
    Glacier,
    Noir,
    Graphite,
}

impl From<Theme> for gtl_models::viewer::Theme {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::Dark,
            Theme::Light => Self::Light,
            Theme::Hearth => Self::Hearth,
            Theme::Mirage => Self::Mirage,
            Theme::Glacier => Self::Glacier,
            Theme::Noir => Self::Noir,
            Theme::Graphite => Self::Graphite,
        }
    }
}

/// Arguments for `status`. Default scope is the current repo; `--all` and `-r` widen it.
#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Report every active project listed by sample_project.
    #[arg(long, conflicts_with = "recursive")]
    pub all: bool,
    /// Report the current repo plus any nested subrepos under the current directory (linked
    /// worktrees are skipped).
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

impl From<LsArgs> for StatusArgs {
    fn from(value: LsArgs) -> Self {
        Self {
            all: true,
            recursive: false,
            read: value.read,
        }
    }
}

/// Flags shared by read-only managed-repo status output.
#[derive(Debug, Clone, Copy, Args)]
pub struct ManagedReadArgs {
    /// Emit machine-readable JSON instead of human text.
    #[arg(long)]
    pub json: bool,
    /// When to emit ANSI colors in human output.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
}

/// ANSI color policy for human output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    /// Color only when stdout is a terminal.
    Auto,
    /// Always emit ANSI color codes.
    Always,
    /// Never emit ANSI color codes.
    Never,
}

/// Arguments for `switch`.
#[derive(Debug, Args)]
pub struct SwitchArgs {
    /// Fast-forward the target branch onto the current branch's commits after switching.
    #[arg(long, conflicts_with = "revert")]
    pub rebase: bool,
    /// Branch to switch to / fast-forward onto / revert (default: main).
    #[arg(long)]
    pub onto: Option<String>,
    /// After rebasing, render an HTML diff of the now-unpushed commits (requires --rebase).
    #[arg(short = 'd', long = "diff", requires = "rebase")]
    pub diff: bool,
    /// Undo the last `switch --rebase`: reset the target branch and switch back to the previous
    /// branch.
    #[arg(short = 'r', long = "revert")]
    pub revert: bool,
}

/// Arguments for `prune`.
#[derive(Debug, Args)]
pub struct PruneArgs {
    /// Integration branch that branches must be merged into to qualify (default: main).
    #[arg(long)]
    pub onto: Option<String>,
    /// Actually delete (skip the prompt; required to delete in a non-interactive shell).
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
    /// Prune every active project listed by sample_project instead of the current repo.
    #[arg(long)]
    pub all: bool,
    /// Emit machine-readable JSON instead of human text (with `--all`).
    #[arg(long, requires = "all")]
    pub json: bool,
}

/// Flags shared by the managed-repo fan-out commands (`push --all`, `pull --all`, `commit --all`).
#[derive(Debug, Clone, Copy, Args)]
pub struct ManagedArgs {
    /// Preview actions without performing them.
    #[arg(long, requires = "all")]
    pub dry: bool,
    /// Emit machine-readable JSON instead of human text.
    #[arg(long, requires = "all")]
    pub json: bool,
}

pub use gtl_application::diffs::DiffTarget;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_tag_bump_accepts_short_aliases() {
        let cli = Cli::parse_args(&[
            "tag".into(),
            "b".into(),
            "patch".into(),
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
                    push: true,
                    dry: false,
                    yes: false,
                }),
                ..
            }) if message == "release"
        ));
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

    #[test]
    fn parse_args_managed_flags_require_all() {
        for command in ["push", "commit", "pull"] {
            for flag in ["--dry", "--json"] {
                assert!(Cli::parse_args(&[command.into(), flag.into()]).is_err());
                assert!(Cli::parse_args(&[command.into(), "--all".into(), flag.into()]).is_ok());
            }
        }
    }

    #[test]
    fn parse_args_prune_json_requires_all() {
        assert!(Cli::parse_args(&["prune".into(), "--json".into()]).is_err());
        assert!(Cli::parse_args(&["prune".into(), "--all".into(), "--json".into()]).is_ok());
    }

    #[test]
    fn parse_args_switch_scenarios() {
        assert!(Cli::parse_args(&["switch".into(), "--diff".into()]).is_err());
        assert!(Cli::parse_args(&["sw".into(), "--rebase".into(), "-d".into()]).is_ok());
        assert!(Cli::parse_args(&["switch".into(), "--rebase".into(), "--revert".into()]).is_err());
    }

    #[test]
    fn parse_args_worktree_accepts_short_alias() {
        for command in ["worktree", "wk"] {
            assert!(Cli::parse_args(&[command.into(), "base".into()]).is_ok());
        }
    }
}
