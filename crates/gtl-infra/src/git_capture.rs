use std::path::Path;

use anyhow::{Context as _, anyhow};
use gtl_application::ports::{GitDiffFormat, GitDiffRequest};
use gtl_models::{
    diffs::{Commit, CommitId, ExtensionSelection, FileExtensions},
    git::{GitDiffSpec, GitRange, GitRevision},
    timestamps::MachineTimestamp,
};

pub(crate) fn run_git(repo_path: impl AsRef<Path>, args: &[&str]) -> anyhow::Result<String> {
    let output = crate::git_process::run(repo_path.as_ref(), args)?;
    if !output.success() {
        let stderr = output.stderr.trim().to_string();
        if stderr.is_empty() {
            return Err(anyhow!("git exited with {}", output.exit_code));
        }
        return Err(anyhow!(stderr));
    }

    Ok(output.stdout)
}

pub(crate) fn log_commits(
    repo_path: impl AsRef<Path>,
    range: &GitRange,
) -> anyhow::Result<Vec<Commit>> {
    let raw = run_git(
        repo_path,
        &[
            "log",
            "--format=%H%x1f%s%x1f%b%x1f%aI%x1f%P%x1e",
            range.as_ref(),
        ],
    )?;
    parse_commit_log(&raw)
}

pub(crate) fn diff(
    repo_path: impl AsRef<Path>,
    request: &GitDiffRequest,
) -> anyhow::Result<String> {
    let repo_path = repo_path.as_ref();
    let pathspecs = match &request.paths {
        ExtensionSelection::Listed(extensions) => {
            let pathspecs = extension_pathspecs("glob,icase", extensions);
            if pathspecs.is_empty() {
                return Ok(String::new());
            }
            pathspecs
        }
        ExtensionSelection::Unlisted(extensions) => {
            extension_pathspecs("exclude,glob,icase", extensions)
        }
    };
    let temporary_index = match &request.spec {
        GitDiffSpec::AgainstWorkingTree(_) => Some(working_tree_index(repo_path)?),
        GitDiffSpec::Range(_) => None,
    };
    // Pin the output the parser reads against user configuration such as `diff.noprefix`,
    // `diff.mnemonicPrefix`, `color.ui=always`, and `diff.external`.
    let mut args = [
        "diff",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--no-color",
        "--no-ext-diff",
    ]
    .map(String::from)
    .to_vec();
    match request.format {
        GitDiffFormat::NamesOnly => args.push("--name-only".to_string()),
        GitDiffFormat::Unified => {}
        GitDiffFormat::FullContext => args.push("--unified=2147483647".to_string()),
    }
    let base = match &request.spec {
        GitDiffSpec::AgainstWorkingTree(revision)
            if *revision == GitRevision::head()
                && !crate::git_process::run(
                    repo_path,
                    &["rev-parse", "--verify", "--quiet", "HEAD"],
                )?
                .success() =>
        {
            run_git(repo_path, &["hash-object", "-t", "tree", "--stdin"])?
                .trim()
                .to_owned()
        }
        _ => request.spec.to_string(),
    };
    args.push(base);
    if !pathspecs.is_empty() {
        args.push("--".to_string());
        args.extend(pathspecs);
    }
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let index = temporary_index
        .as_ref()
        .map(|directory| directory.path().join("index"));
    let output = crate::git_process::run_with_index(repo_path, &args, index.as_deref())?;
    anyhow::ensure!(output.success(), "{}", output.error_line());
    Ok(output.stdout)
}

/// Builds one pathspec per extension that matches exactly the paths
/// [`FileExtensions::contains_extension_of`] accepts, so the argument count stays independent of
/// the number of changed files.
fn extension_pathspecs(magic: &str, extensions: &FileExtensions) -> Vec<String> {
    extensions
        .extensions()
        .iter()
        // A final extension never contains a dot or a separator, so these match no path.
        .filter(|extension| !extension.contains(['.', '/']))
        .map(|extension| {
            // `?*` demands a file stem: a bare dotfile such as `.lock` has no extension.
            let mut pathspec = format!(":({magic})**/?*.");
            for character in extension.chars() {
                if matches!(character, '*' | '?' | '[' | '\\') {
                    pathspec.push('\\');
                }
                pathspec.push(character);
            }
            pathspec
        })
        .collect()
}

fn working_tree_index(repo_path: &Path) -> anyhow::Result<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    let index = directory.path().join("index");
    let original = run_git(
        repo_path,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    let original = Path::new(original.trim());
    if original.exists() {
        std::fs::copy(original, &index)?;
    } else {
        let output =
            crate::git_process::run_with_index(repo_path, &["read-tree", "--empty"], Some(&index))?;
        anyhow::ensure!(output.success(), "{}", output.error_line());
    }
    let output = crate::git_process::run_with_index(
        repo_path,
        &["add", "--intent-to-add", "--all", "--", "."],
        Some(&index),
    )?;
    anyhow::ensure!(output.success(), "{}", output.error_line());
    Ok(directory)
}

/// The repository's oldest root commit (lexicographically smallest when several
/// roots exist), or `None` for a repository with no commits.
pub(crate) fn root_commit(repo_path: impl AsRef<Path>) -> Option<CommitId> {
    let out = run_git(repo_path, &["rev-list", "--max-parents=0", "HEAD"]).ok()?;
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .min()
        .and_then(|id| id.try_into().ok())
}

/// The validated merge base of `a` and `b`.
pub(crate) fn merge_base(
    repo_path: impl AsRef<Path>,
    a: &GitRevision,
    b: &GitRevision,
) -> anyhow::Result<CommitId> {
    run_git(repo_path, &["merge-base", a.as_ref(), b.as_ref()])?
        .trim()
        .try_into()
        .map_err(Into::into)
}

pub(crate) fn parse_commit_log(raw: &str) -> anyhow::Result<Vec<Commit>> {
    raw.split('\x1e')
        .map(str::trim)
        .filter(|record| !record.is_empty())
        .map(|record| {
            let mut fields = record.split('\x1f');
            let id = fields
                .next()
                .context("Git log entry is missing a commit ID")?;
            let subject = fields
                .next()
                .context("Git log entry is missing a subject")?;
            let body = fields.next().context("Git log entry is missing a body")?;
            let committed_at = fields
                .next()
                .context("Git log entry is missing an author timestamp")?;
            let parents = fields.next().context("Git log entry is missing parents")?;
            Ok(Commit {
                id: id
                    .try_into()
                    .context("Git log entry has an invalid commit ID")?,
                subject: subject.to_owned(),
                body: body.to_owned(),
                committed_at: MachineTimestamp::try_from(committed_at)
                    .context("Git log entry has an invalid author timestamp")?,
                parents: parents
                    .split_whitespace()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()
                    .context("Git log entry has an invalid parent commit ID")?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestRepository;

    const COMMIT_ID_ONE: &str = "1111111111111111111111111111111111111111";
    const COMMIT_ID_TWO: &str = "2222222222222222222222222222222222222222";
    const COMMIT_ID_THREE: &str = "3333333333333333333333333333333333333333";
    const COMMIT_ID_FOUR: &str = "4444444444444444444444444444444444444444";

    #[test]
    fn parse_commit_log_retains_full_commit_identities() {
        let raw = concat!(
            "1111111111111111111111111111111111111111\x1fadd renderer\x1fbody text\nmore body\x1f2026-06-08T13:45:00-03:00\x1f\x1e",
            "2222222222222222222222222222222222222222\x1ffix parser\x1f\x1f2026-06-09T09:10:00-03:00\x1f\x1e",
        );

        let commits = parse_commit_log(raw).unwrap();

        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].id.to_string(), COMMIT_ID_ONE);
        assert_eq!(commits[0].subject, "add renderer");
        assert_eq!(commits[0].body, "body text\nmore body");
        assert_eq!(commits[0].committed_at.display_minute(), "2026-06-08 13:45");
        assert_eq!(
            commits[0].committed_at.as_ref(),
            "2026-06-08T13:45:00-03:00"
        );
        assert_eq!(commits[1].id.to_string(), COMMIT_ID_TWO);
        assert_eq!(commits[1].subject, "fix parser");
        assert_eq!(commits[1].body, "");
    }

    #[test]
    fn parse_commit_log_rejects_missing_machine_timestamp() {
        let error = parse_commit_log(concat!(
            " \n\x1e",
            "3333333333333333333333333333333333333333\x1fsubject only\x1e"
        ))
        .unwrap_err();

        assert!(error.to_string().contains("missing a body"));
    }

    #[test]
    fn parse_commit_log_reads_parents_and_flags_merge() {
        // Record fields: SHA, subject, body, ISO timestamp, parents (space-separated).
        let raw = concat!(
            "1111111111111111111111111111111111111111\x1fMerge branch 'sub'\x1f\x1f2026-06-08T13:45:00-03:00\x1f3333333333333333333333333333333333333333 4444444444444444444444444444444444444444\x1e",
            "2222222222222222222222222222222222222222\x1ffeat: x\x1f\x1f2026-06-09T09:10:00-03:00\x1f3333333333333333333333333333333333333333\x1e",
        );

        let commits = parse_commit_log(raw).unwrap();

        assert_eq!(
            commits[0]
                .parents
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [COMMIT_ID_THREE, COMMIT_ID_FOUR]
        );
        assert!(commits[0].is_merge());
        assert_eq!(commits[1].parents[0].to_string(), COMMIT_ID_THREE);
        assert!(!commits[1].is_merge());
    }

    #[test]
    fn parse_commit_log_rejects_invalid_git_output_identity() {
        let error =
            parse_commit_log("not-a-commit-id\x1fsubject\x1f\x1f2026-06-08T13:45:00-03:00\x1f\x1e")
                .unwrap_err();

        assert!(error.to_string().contains("invalid commit ID"));
    }

    #[test]
    fn parse_commit_log_rejects_timezone_less_timestamp() {
        let error = parse_commit_log(
            "1111111111111111111111111111111111111111\x1fsubject\x1f\x1f2026-06-08T13:45:00\x1f\x1e",
        )
        .unwrap_err();

        assert!(error.to_string().contains("invalid author timestamp"));
    }

    #[test]
    fn extension_pathspecs_select_the_paths_the_extension_rule_selects() {
        let repository = TestRepository::new();
        repository.git(&["config", "core.quotePath", "false"]);
        repository.commit_all("base");
        let paths = [
            "main.rs",
            "src/nested/lib.RS",
            ".rs",
            "..rs",
            "Cargo.lock",
            ".lock",
            "dir.lock/inner.rs",
            "dir.rs/Makefile",
            "Makefile",
            "bundle.tar.gz",
            "x.äb",
            "x.ÄB",
            "glob.a*",
            "glob.ab",
        ];
        for path in paths {
            repository.write(path, "content\n");
        }
        repository.commit_all("files");
        let spec = GitDiffSpec::Range(GitRange::try_new("HEAD~1..HEAD").unwrap());
        let listed =
            |extensions: &[&str]| ExtensionSelection::Listed(FileExtensions::new(extensions));
        let unlisted =
            |extensions: &[&str]| ExtensionSelection::Unlisted(FileExtensions::new(extensions));

        for selection in [
            listed(&["rs"]),
            unlisted(&["lock"]),
            listed(&["lock", "rs"]),
            unlisted(&["lock", "rs"]),
            listed(&["äb"]),
            unlisted(&["äb"]),
            listed(&["a*"]),
            unlisted(&["tar.gz"]),
            listed(&["tar.gz"]),
            ExtensionSelection::all(),
        ] {
            let output = diff(
                repository.path(),
                &GitDiffRequest {
                    spec: spec.clone(),
                    format: GitDiffFormat::NamesOnly,
                    paths: selection.clone(),
                },
            )
            .unwrap();
            let mut selected_by_git = output.lines().collect::<Vec<_>>();
            selected_by_git.sort_unstable();
            let mut selected_by_rule = paths
                .into_iter()
                .filter(|path| {
                    selection.contains(
                        &gtl_models::paths::RepositoryRelativePath::try_new((*path).into())
                            .unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            selected_by_rule.sort_unstable();
            assert_eq!(selected_by_git, selected_by_rule, "{selection:?}");
        }
    }

    #[test]
    fn root_commit_returns_oldest_root_sha() {
        let repository = TestRepository::new();
        repository.write("a.txt", "a\n");
        let head = repository.commit_all("first");
        let root = root_commit(repository.path()).unwrap();
        assert_eq!(root.as_ref().len(), 40);
        assert_eq!(root.as_ref(), head); // single commit ⇒ root == HEAD
    }
}
