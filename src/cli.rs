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

/// git-tools — render git workflow HTML previews and squash local commits.
#[derive(Debug, Parser)]
#[command(name = "git-tools", version, about, long_about = None, arg_required_else_help = true)]
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
        let argv = std::iter::once(String::from("git-tools")).chain(args.iter().cloned());
        Self::try_parse_from(argv)
    }
}

/// Top-level subcommands; each maps to one [`crate::commands`] entry point.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Render a squash preview of a subrepo's unpushed work against its monorepo.
    SquashPreview {
        /// Subrepo working tree to preview.
        #[arg(long)]
        repo: String,
        /// Monorepo root used to resolve the subrepo path (preview is written to the central store).
        #[arg(long)]
        monorepo: String,
    },
    /// Render an HTML diff of the current repo, or all managed repos with `--all`.
    Diff(DiffArgs),
    /// Render a merge preview (three-dot diff) of a subrepo against a base branch.
    MergeDiff {
        /// Subrepo working tree to preview.
        #[arg(long)]
        repo: String,
        /// Monorepo root used to resolve the subrepo path (preview is written to the central store).
        #[arg(long)]
        monorepo: String,
        /// Base branch to merge into (default: main).
        #[arg(long)]
        base: Option<String>,
    },
    /// Squash all unpushed local commits into a single commit.
    SquashLocal {
        /// New commit message for the squashed commit.
        #[arg(allow_hyphen_values = true)]
        message: String,
        /// Repo whose unpushed commits are squashed.
        #[arg(long)]
        repo: String,
        /// Preview the squash without rewriting history.
        #[arg(long)]
        dry: bool,
    },
    /// Stage all changes, commit, and push the current repo (prompts for confirmation first).
    Up {
        /// Commit message for the staged changes.
        message: String,
        /// Skip the confirmation prompt (for non-interactive use, e.g. a justfile recipe).
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },
    /// List tags, show tag commits, or push tags.
    Tag(TagArgs),
    /// Inspect git worktrees.
    Wk(WorktreeArgs),
    /// Switch to the main branch; with `--rebase`, fast-forward it onto the current branch's commits.
    Sw(SwArgs),
    /// Show branch, unpushed commits, and pending changes for every managed repo.
    #[command(visible_alias = "ls")]
    Status(ManagedReadArgs),
    /// Push every managed repo that has unpushed commits.
    PushAll(ManagedArgs),
    /// Pull every managed repo.
    PullAll(ManagedArgs),
    /// Commit pending changes across every managed repo.
    CommitAll {
        #[command(flatten)]
        managed: ManagedArgs,
        /// Commit message to apply to every repo with pending changes.
        #[arg(long)]
        message_for_all: Option<String>,
    },
}

/// Arguments for the root `diff` command and its nested subcommands.
#[derive(Debug, Args)]
pub struct DiffArgs {
    #[command(subcommand)]
    pub command: Option<DiffCommand>,
    #[command(flatten)]
    pub target: DiffTargetArgs,
}

/// Nested commands under `diff`.
#[derive(Debug, Subcommand)]
pub enum DiffCommand {
    /// Render one tabbed HTML diff for every git repo under the current directory.
    Subrepos(DiffSubreposScanArgs),
}

/// Arguments for `tag`.
#[derive(Debug, Args)]
pub struct TagArgs {
    #[command(subcommand)]
    pub command: Option<TagCommand>,
    /// Show the commit each tag points at.
    #[arg(short = 'c', long = "commits")]
    pub commits: bool,
}

/// Nested commands under `tag`.
#[derive(Debug, Subcommand)]
pub enum TagCommand {
    /// List local tags and whether each is already known on origin.
    Ls,
    /// Create an annotated tag.
    Add {
        /// Tag name to create.
        tag: String,
        /// Annotated tag message.
        message: String,
    },
    /// Push local tags, or create one annotated tag and push it.
    Up {
        /// Optional tag name to create before pushing.
        tag: Option<String>,
        /// Annotated tag message when creating a tag.
        message: Option<String>,
    },
}

/// Arguments for `wk`.
#[derive(Debug, Args)]
pub struct WorktreeArgs {
    #[command(subcommand)]
    pub command: WorktreeCommand,
}

/// Nested commands under `wk`.
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
    /// Render one tabbed HTML diff for all managed repos with unpushed commits.
    #[arg(long, conflicts_with_all = ["target", "last", "unpushed"])]
    pub all: bool,
    /// Diff unpushed work (`@{u}..HEAD`); this is also the default when no target is supplied.
    #[arg(long, conflicts_with_all = ["target", "last"])]
    pub unpushed: bool,
    /// Base commit, a `<start>..<end>` range, or empty/omitted for unpushed work.
    #[arg(conflicts_with = "last")]
    pub target: Option<String>,
    /// Diff the last N commits (`HEAD~N..HEAD`); bare `-l` diffs the last commit.
    #[arg(short = 'l', long = "last", value_name = "N", num_args = 0..=1, default_missing_value = "1")]
    pub last: Option<NonZeroU32>,
    /// Diff what merging HEAD into BASE would introduce (`BASE...HEAD`).
    #[arg(short = 'm', long = "merge", value_name = "BASE", conflicts_with_all = ["target", "last", "unpushed", "all"])]
    pub merge: Option<String>,
    /// Name the generated diff in the viewer history label.
    #[arg(short = 'n', long = "name", value_name = "NAME", value_parser = non_empty_name)]
    pub name: Option<String>,
    /// Path to the managed-repos manifest (overrides the default lookup).
    #[arg(long, requires = "all")]
    pub repos_file: Option<String>,
    /// Home directory used to resolve managed-repo paths (overrides `$HOME`).
    #[arg(long, requires = "all")]
    pub home_dir: Option<String>,
}

/// Flags for `diff subrepos`.
#[derive(Debug, Args)]
pub struct DiffSubreposScanArgs {
    /// Diff the last N commits in every discovered repo; bare `-l` diffs the last commit.
    #[arg(short = 'l', long = "last", value_name = "N", num_args = 0..=1, default_missing_value = "1")]
    pub last: Option<NonZeroU32>,
    /// Include nested linked worktrees (e.g. `.worktrees/<name>`); skipped by default.
    #[arg(short = 'w', long = "worktrees", visible_alias = "wk")]
    pub worktrees: bool,
}

/// Flags shared by read-only managed-repo commands (`status`/`ls`).
#[derive(Debug, Args)]
pub struct ManagedReadArgs {
    /// Emit machine-readable JSON instead of human text.
    #[arg(long)]
    pub json: bool,
    /// When to emit ANSI colors in human output.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
    /// Path to the managed-repos manifest (overrides the default lookup).
    #[arg(long)]
    pub repos_file: Option<String>,
    /// Home directory used to resolve managed-repo paths (overrides `$HOME`).
    #[arg(long)]
    pub home_dir: Option<String>,
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

/// Arguments for `sw`.
#[derive(Debug, Args)]
pub struct SwArgs {
    /// Fast-forward the target branch onto the current branch's commits after switching.
    #[arg(long, conflicts_with = "revert")]
    pub rebase: bool,
    /// Branch to switch to / fast-forward onto / revert (default: main).
    #[arg(long)]
    pub onto: Option<String>,
    /// After rebasing, render an HTML diff of the now-unpushed commits (requires --rebase).
    #[arg(short = 'd', long = "diff", requires = "rebase")]
    pub diff: bool,
    /// Undo the last `sw --rebase`: reset the target branch and switch back to the previous branch.
    #[arg(short = 'r', long = "revert")]
    pub revert: bool,
}

/// Flags shared by the managed-repo fan-out commands (`push-all`, `pull-all`, `commit-all`).
#[derive(Debug, Args)]
pub struct ManagedArgs {
    /// Preview actions without performing them.
    #[arg(long)]
    pub dry: bool,
    /// Emit machine-readable JSON instead of human text.
    #[arg(long)]
    pub json: bool,
    /// Path to the managed-repos manifest (overrides the default lookup).
    #[arg(long)]
    pub repos_file: Option<String>,
    /// Home directory used to resolve managed-repo paths (overrides `$HOME`).
    #[arg(long)]
    pub home_dir: Option<String>,
}

/// What a `diff` invocation targets, resolved from its optional positional argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffTarget {
    /// Unpushed work: `@{u}..HEAD`.
    Unpushed,
    /// A single base commit diffed against the working tree.
    Base(String),
    /// An exact `<start>..<end>` commit range.
    Range(String),
    /// A three-dot merge preview against the base branch.
    Merge(String),
    /// The last N commits (`HEAD~N..HEAD`).
    Last(NonZeroU32),
}

impl DiffTarget {
    /// Resolves the `diff` positional into a target: empty/absent is unpushed work, a value
    /// containing `..` is an exact range, anything else is a base commit.
    pub fn from_arg(arg: Option<&str>) -> Self {
        match arg {
            None => Self::Unpushed,
            Some(value) if value.trim().is_empty() => Self::Unpushed,
            Some(value) if value.contains("..") => Self::Range(value.to_string()),
            Some(value) => Self::Base(value.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_target_empty_or_absent_is_unpushed() {
        assert_eq!(DiffTarget::from_arg(None), DiffTarget::Unpushed);
        assert_eq!(DiffTarget::from_arg(Some("")), DiffTarget::Unpushed);
        assert_eq!(DiffTarget::from_arg(Some("   ")), DiffTarget::Unpushed);
    }

    #[test]
    fn diff_target_distinguishes_range_from_base() {
        assert_eq!(
            DiffTarget::from_arg(Some("abc123..def456")),
            DiffTarget::Range("abc123..def456".to_string())
        );
        assert_eq!(
            DiffTarget::from_arg(Some("abc123")),
            DiffTarget::Base("abc123".to_string())
        );
    }

    #[test]
    fn parse_args_routes_canonical_subcommands() {
        let cli = Cli::parse_args(&[
            "squash-preview".into(),
            "--repo".into(),
            "r".into(),
            "--monorepo".into(),
            "m".into(),
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::SquashPreview { repo, monorepo } if repo == "r" && monorepo == "m"
        ));
    }

    #[test]
    fn parse_args_routes_worktree_commands() {
        let cli = Cli::parse_args(&["wk".into(), "base".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Wk(WorktreeArgs {
                command: WorktreeCommand::Base
            })
        ));

        let cli = Cli::parse_args(&["wk".into(), "ls".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Wk(WorktreeArgs {
                command: WorktreeCommand::Ls
            })
        ));
    }

    #[test]
    fn parse_args_accepts_dash_prefixed_squash_message() {
        // A commit message can begin with `-`; allow_hyphen_values keeps it a value, not a flag.
        let cli = Cli::parse_args(&[
            "squash-local".into(),
            "- fix commit".into(),
            "--repo".into(),
            "r".into(),
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::SquashLocal { message, repo, dry }
                if message == "- fix commit" && repo == "r" && !dry
        ));
    }

    #[test]
    fn parse_args_rejects_unknown_command() {
        assert!(Cli::parse_args(&["bogus".into()]).is_err());
    }

    #[test]
    fn parse_args_rejects_explicit_flags_on_lean_diff() {
        assert!(Cli::parse_args(&["diff".into(), "--repo".into(), "r".into()]).is_err());
    }

    #[test]
    fn parse_args_diff_last_takes_a_count() {
        let cli = Cli::parse_args(&["diff".into(), "-l".into(), "5".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Diff(DiffArgs {
                command: None,
                target: DiffTargetArgs {
                    target: None,
                    last: Some(n),
                    ..
                },
            }) if n.get() == 5
        ));
    }

    #[test]
    fn parse_args_diff_bare_last_defaults_to_one() {
        let cli = Cli::parse_args(&["diff".into(), "-l".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Diff(DiffArgs {
                command: None,
                target: DiffTargetArgs {
                    target: None,
                    last: Some(n),
                    ..
                },
            }) if n.get() == 1
        ));
    }

    #[test]
    fn parse_args_diff_merge_sets_base() {
        let cli = Cli::parse_args(&["diff".into(), "-m".into(), "main".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Diff(DiffArgs {
                command: None,
                target: DiffTargetArgs {
                    merge: Some(base),
                    target: None,
                    last: None,
                    ..
                },
            }) if base == "main"
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
    fn parse_args_diff_rejects_blank_name() {
        assert!(Cli::parse_args(&["diff".into(), "--name".into(), "  ".into()]).is_err());
    }

    #[test]
    fn parse_args_diff_subrepos_bare_last_defaults_to_one() {
        let cli = Cli::parse_args(&["diff".into(), "subrepos".into(), "-l".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Diff(DiffArgs {
                command: Some(DiffCommand::Subrepos(DiffSubreposScanArgs { last: Some(n), .. })),
                target: DiffTargetArgs {
                    target: None,
                    last: None,
                    ..
                },
            }) if n.get() == 1
        ));
    }

    #[test]
    fn parse_args_diff_all_accepts_managed_overrides() {
        let cli = Cli::parse_args(&[
            "diff".into(),
            "--all".into(),
            "--repos-file".into(),
            "repos.txt".into(),
            "--home-dir".into(),
            "/tmp/home".into(),
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Diff(DiffArgs {
                command: None,
                target: DiffTargetArgs {
                    all: true,
                    repos_file: Some(repos_file),
                    home_dir: Some(home_dir),
                    target: None,
                    last: None,
                    ..
                },
            }) if repos_file == "repos.txt" && home_dir == "/tmp/home"
        ));
    }

    #[test]
    fn parse_args_diff_rejects_zero_last() {
        // NonZeroU32 makes `-l 0` unrepresentable: clap rejects it before dispatch.
        assert!(Cli::parse_args(&["diff".into(), "-l".into(), "0".into()]).is_err());
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
    fn parse_args_routes_up_with_message() {
        let cli = Cli::parse_args(&["up".into(), "save work".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Up { message, yes } if message == "save work" && !yes
        ));
    }

    #[test]
    fn parse_args_up_yes_flag_sets_bypass() {
        let cli = Cli::parse_args(&["up".into(), "save work".into(), "--yes".into()]).unwrap();
        assert!(matches!(cli.command, Command::Up { yes, .. } if yes));
    }

    #[test]
    fn parse_args_up_requires_a_message() {
        assert!(Cli::parse_args(&["up".into()]).is_err());
    }

    #[test]
    fn parse_args_rejects_legacy_sync_command() {
        assert!(Cli::parse_args(&["sync".into(), "save work".into()]).is_err());
    }

    #[test]
    fn parse_args_routes_tag_list_by_default() {
        let cli = Cli::parse_args(&["tag".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Tag(TagArgs {
                command: None,
                commits: false
            })
        ));
    }

    #[test]
    fn parse_args_routes_tag_commits_flag() {
        let cli = Cli::parse_args(&["tag".into(), "--commits".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Tag(TagArgs {
                command: None,
                commits: true
            })
        ));
    }

    #[test]
    fn parse_args_routes_tag_commits_short_flag() {
        let cli = Cli::parse_args(&["tag".into(), "-c".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Tag(TagArgs {
                command: None,
                commits: true
            })
        ));
    }

    #[test]
    fn parse_args_routes_tag_up_subcommand() {
        let cli = Cli::parse_args(&["tag".into(), "up".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Tag(TagArgs {
                command: Some(TagCommand::Up {
                    tag: None,
                    message: None
                }),
                commits: false
            })
        ));
    }

    #[test]
    fn parse_args_routes_tag_ls_subcommand() {
        let cli = Cli::parse_args(&["tag".into(), "ls".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Tag(TagArgs {
                command: Some(TagCommand::Ls),
                commits: false
            })
        ));
    }

    #[test]
    fn parse_args_routes_tag_add_subcommand() {
        let cli = Cli::parse_args(&[
            "tag".into(),
            "add".into(),
            "v1.2.0".into(),
            "release notes".into(),
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Tag(TagArgs {
                command: Some(TagCommand::Add { tag, message }),
                commits: false
            }) if tag == "v1.2.0" && message == "release notes"
        ));
    }

    #[test]
    fn parse_args_routes_tag_up_create_form() {
        let cli = Cli::parse_args(&[
            "tag".into(),
            "up".into(),
            "v1.2.0".into(),
            "release notes".into(),
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Tag(TagArgs {
                command: Some(TagCommand::Up {
                    tag: Some(tag),
                    message: Some(message),
                }),
                commits: false
            }) if tag == "v1.2.0" && message == "release notes"
        ));
    }

    #[test]
    fn parse_args_routes_sw_switch_only_by_default() {
        let cli = Cli::parse_args(&["sw".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Sw(SwArgs {
                rebase: false,
                revert: false,
                diff: false,
                onto: None
            })
        ));
    }

    #[test]
    fn parse_args_routes_sw_rebase_with_onto() {
        let cli = Cli::parse_args(&[
            "sw".into(),
            "--rebase".into(),
            "--onto".into(),
            "trunk".into(),
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Sw(SwArgs { rebase: true, onto: Some(onto), .. }) if onto == "trunk"
        ));
    }

    #[test]
    fn parse_args_sw_diff_requires_rebase() {
        assert!(Cli::parse_args(&["sw".into(), "--diff".into()]).is_err());
        assert!(Cli::parse_args(&["sw".into(), "--rebase".into(), "-d".into()]).is_ok());
    }

    #[test]
    fn parse_args_sw_rebase_conflicts_with_revert() {
        assert!(Cli::parse_args(&["sw".into(), "--rebase".into(), "--revert".into()]).is_err());
    }

    #[test]
    fn parse_args_routes_sw_revert() {
        let cli = Cli::parse_args(&["sw".into(), "-r".into()]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Sw(SwArgs { revert: true, .. })
        ));
    }
}
