//! CLI surface, parsed with clap-derive. clap owns argument parsing, `--help`, and
//! `--version`. Each variant/field doc comment is the single source of truth for its help text.

use clap::{Args, Parser, Subcommand};

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
        /// Monorepo root the preview is written under (`.artifacts/`).
        #[arg(long)]
        monorepo: String,
    },
    /// Render an HTML diff of the current repo: unpushed work, a base commit, or a `<start>..<end>` range.
    Diff {
        /// Base commit, a `<start>..<end>` range, or empty/omitted for unpushed work.
        target: Option<String>,
    },
    /// Render a per-subrepo diff for a monorepo's subrepos.
    DiffSubrepos {
        /// Subrepo working tree to diff.
        #[arg(long)]
        repo: String,
        /// Monorepo root the preview is written under (`.artifacts/`).
        #[arg(long)]
        monorepo: String,
        /// Base ref to diff from; omit for unpushed work.
        #[arg(long)]
        base: Option<String>,
    },
    /// Render a merge preview (three-dot diff) of a subrepo against a base branch.
    MergeDiff {
        /// Subrepo working tree to preview.
        #[arg(long)]
        repo: String,
        /// Monorepo root the preview is written under (`.artifacts/`).
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
}
