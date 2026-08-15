use std::path::Path;

use anyhow::anyhow;
use gtl_application::ports::{GitDiffFormat, GitDiffRequest};
use gtl_models::diffs::{Commit, CommitId, CommitIdError};

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

pub(crate) fn log_commits(repo_path: impl AsRef<Path>, range: &str) -> anyhow::Result<Vec<Commit>> {
    let raw = run_git(
        repo_path,
        &[
            "log",
            "--date=format:%Y-%m-%d %H:%M",
            "--format=%H%x1f%s%x1f%b%x1f%ad%x1f%aI%x1f%P%x1e",
            range,
        ],
    )?;
    parse_commit_log(&raw).map_err(Into::into)
}

pub(crate) fn diff(
    repo_path: impl AsRef<Path>,
    request: &GitDiffRequest,
) -> anyhow::Result<String> {
    let mut args = vec!["diff".to_string()];
    match request.format {
        GitDiffFormat::NamesOnly => args.push("--name-only".to_string()),
        GitDiffFormat::Unified => {}
        GitDiffFormat::FullContext => args.push("--unified=2147483647".to_string()),
    }
    args.push(request.range.clone());
    if !request.excluded_paths.is_empty() {
        args.push("--".to_string());
        args.extend(
            request
                .excluded_paths
                .iter()
                .map(|path| format!(":(exclude,literal){path}")),
        );
    }
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_git(repo_path, &args)
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
    a: &str,
    b: &str,
) -> anyhow::Result<CommitId> {
    run_git(repo_path, &["merge-base", a, b])?
        .trim()
        .try_into()
        .map_err(Into::into)
}

/// The committer date of `rev` as a strict ISO-8601 string (empty on failure).
pub(crate) fn committed_at(repo_path: impl AsRef<Path>, rev: &str) -> String {
    run_git(repo_path, &["show", "-s", "--format=%cI", rev])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub(crate) fn parse_commit_log(raw: &str) -> Result<Vec<Commit>, CommitIdError> {
    raw.split('\x1e')
        .map(str::trim)
        .filter(|record| !record.is_empty())
        .map(|record| {
            let mut fields = record.split('\x1f');
            Ok(Commit {
                id: fields.next().unwrap_or_default().try_into()?,
                subject: fields.next().unwrap_or("").to_string(),
                body: fields.next().unwrap_or("").to_string(),
                date: fields.next().unwrap_or("").to_string(),
                iso: fields.next().unwrap_or("").to_string(),
                parents: fields
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
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
            "1111111111111111111111111111111111111111\x1fadd renderer\x1fbody text\nmore body\x1f2026-06-08 13:45\x1f2026-06-08T13:45:00-03:00\x1e",
            "2222222222222222222222222222222222222222\x1ffix parser\x1f\x1f2026-06-09 09:10\x1f2026-06-09T09:10:00-03:00\x1e",
        );

        let commits = parse_commit_log(raw).expect("valid commit log");

        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].id.to_string(), COMMIT_ID_ONE);
        assert_eq!(commits[0].subject, "add renderer");
        assert_eq!(commits[0].body, "body text\nmore body");
        assert_eq!(commits[0].date, "2026-06-08 13:45");
        assert_eq!(commits[0].iso, "2026-06-08T13:45:00-03:00");
        assert_eq!(commits[1].id.to_string(), COMMIT_ID_TWO);
        assert_eq!(commits[1].subject, "fix parser");
        assert_eq!(commits[1].body, "");
    }

    #[test]
    fn parse_commit_log_trims_blank_records_and_defaults_missing_fields() {
        let commits = parse_commit_log(concat!(
            " \n\x1e",
            "3333333333333333333333333333333333333333\x1fsubject only\x1e"
        ))
        .expect("valid commit log");

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].id.to_string(), COMMIT_ID_THREE);
        assert_eq!(commits[0].subject, "subject only");
        assert_eq!(commits[0].body, "");
        assert_eq!(commits[0].date, "");
        assert_eq!(commits[0].iso, "");
    }

    #[test]
    fn parse_commit_log_reads_parents_and_flags_merge() {
        // record fields: sha · subject · body · date · iso · parents(space-sep)
        let raw = concat!(
            "1111111111111111111111111111111111111111\x1fMerge branch 'sub'\x1f\x1f2026-06-08 13:45\x1f2026-06-08T13:45:00-03:00\x1f3333333333333333333333333333333333333333 4444444444444444444444444444444444444444\x1e",
            "2222222222222222222222222222222222222222\x1ffeat: x\x1f\x1f2026-06-09 09:10\x1f2026-06-09T09:10:00-03:00\x1f3333333333333333333333333333333333333333\x1e",
        );

        let commits = parse_commit_log(raw).expect("valid commit log");

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
        assert_eq!(
            parse_commit_log("not-a-commit-id\x1fsubject\x1e"),
            Err(CommitIdError)
        );
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
