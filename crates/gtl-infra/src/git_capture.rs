use std::path::Path;

use anyhow::{Context as _, anyhow};
use gtl_application::ports::{GitDiffFormat, GitDiffPaths, GitDiffRequest};
use gtl_models::{
    diffs::{Commit, CommitId},
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
    if matches!(&request.paths, GitDiffPaths::Including(paths) if paths.is_empty()) {
        return Ok(String::new());
    }
    let temporary_index = match &request.spec {
        GitDiffSpec::AgainstWorkingTree(_) => Some(working_tree_index(repo_path)?),
        GitDiffSpec::Range(_) => None,
    };
    let mut args = vec!["diff".to_string()];
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
    let (paths, magic) = match &request.paths {
        GitDiffPaths::Excluding(paths) => (paths, ":(exclude,literal)"),
        GitDiffPaths::Including(paths) => (paths, ":(literal)"),
    };
    if !paths.is_empty() {
        args.push("--".to_string());
        args.extend(
            paths
                .iter()
                .map(|path| format!("{magic}{}", path.display())),
        );
    }
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let index = temporary_index
        .as_ref()
        .map(|directory| directory.path().join("index"));
    let output = crate::git_process::run_with_index(repo_path, &args, index.as_deref())?;
    anyhow::ensure!(output.success(), "{}", output.error_line());
    Ok(output.stdout)
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

/// The committer timestamp of `rev`, or `None` when Git or decoding fails.
pub(crate) fn committed_at(
    repo_path: impl AsRef<Path>,
    rev: &GitRevision,
) -> Option<MachineTimestamp> {
    run_git(repo_path, &["show", "-s", "--format=%cI", rev.as_ref()])
        .ok()
        .and_then(|raw| MachineTimestamp::try_from(raw.trim()).ok())
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
    fn root_commit_returns_oldest_root_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let g = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(d)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        };
        g(&["init", "-q"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        std::fs::write(d.join("a.txt"), "a\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "first"]);
        let root = root_commit(d.to_str().unwrap()).unwrap();
        assert_eq!(root.as_ref().len(), 40);
        let head = run_git(d, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(root.as_ref(), head); // single commit ⇒ root == HEAD
    }
}
