use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, anyhow};

use crate::model::{Commit, FileDiff};

pub fn run_git(repo: impl AsRef<Path>, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo.as_ref())
        .args(args)
        .output()
        .with_context(|| format!("failed to run git in {}", repo.as_ref().display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(anyhow!("git exited with {}", output.status));
        }
        return Err(anyhow!(stderr));
    }

    String::from_utf8(output.stdout).context("git stdout was not valid UTF-8")
}

pub fn top_level(repo: impl AsRef<Path>) -> anyhow::Result<String> {
    run_git(repo.as_ref(), &["rev-parse", "--show-toplevel"])
        .map(|s| s.trim().to_string())
        .map_err(|error| {
            legacy_script_error(
                error.to_string(),
                format!("not a git repo: {}", repo.as_ref().display()),
            )
        })
}

pub fn current_branch(repo: impl AsRef<Path>) -> anyhow::Result<String> {
    run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string())
}

pub fn upstream(repo: impl AsRef<Path>) -> anyhow::Result<String> {
    run_git(
        repo.as_ref(),
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .map(|s| s.trim().to_string())
    .map_err(|error| no_upstream_error(&error.to_string()))
}

pub fn verify_commit(repo: impl AsRef<Path>, base: &str) -> anyhow::Result<()> {
    let rev = format!("{base}^{{commit}}");
    run_git(repo, &["rev-parse", "--verify", &rev])
        .map(|_| ())
        .map_err(|error| legacy_script_error(error.to_string(), format!("not a commit: {base}")))
}

pub fn short_ref(repo: impl AsRef<Path>, base: &str) -> anyhow::Result<String> {
    run_git(repo, &["rev-parse", "--short", base]).map(|s| s.trim().to_string())
}

pub fn log_commits(repo: impl AsRef<Path>, range: &str) -> anyhow::Result<Vec<Commit>> {
    let raw = run_git(
        repo,
        &[
            "log",
            "--date=format:%Y-%m-%d %H:%M",
            "--format=%H%x1f%s%x1f%b%x1f%ad%x1f%aI%x1e",
            range,
        ],
    )?;
    Ok(parse_commit_log(&raw))
}

pub fn file_commit_map(
    repo: impl AsRef<Path>,
    range: &str,
) -> anyhow::Result<HashMap<String, Vec<String>>> {
    let raw = run_git(repo, &["log", "--name-only", "--format=%x1e%H", range])?;
    Ok(parse_file_commit_map(&raw))
}

pub fn attach_commits(files: &mut [FileDiff], map: &HashMap<String, Vec<String>>) {
    for file in files {
        file.commits = map.get(&file.path).cloned().unwrap_or_default();
    }
}

pub fn diff_raw(repo: impl AsRef<Path>, args: &[String]) -> anyhow::Result<String> {
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_git(repo, &args)
}

// ! Range-bounded forward blame of the tip: new-side line -> last commit that touched it.
pub fn blame_forward(
    repo: impl AsRef<Path>,
    base: &str,
    tip: &str,
    path: &str,
) -> anyhow::Result<String> {
    run_git(
        repo,
        &["blame", "--porcelain", &format!("{base}..{tip}"), "--", path],
    )
}

// ! Working-tree blame for hash mode (diff is base -> worktree): aligns with worktree
// ! line numbers; uncommitted lines come back as the all-zero sha (out of range).
pub fn blame_forward_worktree(repo: impl AsRef<Path>, path: &str) -> anyhow::Result<String> {
    run_git(repo, &["blame", "--porcelain", "--", path])
}

// ! Reverse blame over the range: each deleted base line carries `previous <sha>` = its deleter.
pub fn blame_reverse(
    repo: impl AsRef<Path>,
    base: &str,
    tip: &str,
    path: &str,
) -> anyhow::Result<String> {
    run_git(
        repo,
        &[
            "blame",
            "--reverse",
            "--porcelain",
            &format!("{base}..{tip}"),
            "--",
            path,
        ],
    )
}

fn no_upstream_error(git_stderr: &str) -> anyhow::Error {
    legacy_script_error(
        git_stderr,
        "no upstream tracking branch (run: git push -u origin <branch>)",
    )
}

fn legacy_script_error(
    git_stderr: impl AsRef<str>,
    script_message: impl AsRef<str>,
) -> anyhow::Error {
    let git_stderr = git_stderr.as_ref().trim();
    if git_stderr.is_empty() {
        return anyhow!("{}", script_message.as_ref());
    }
    anyhow!("{}\n{}", git_stderr, script_message.as_ref())
}

pub fn parse_commit_log(raw: &str) -> Vec<Commit> {
    raw.split('\x1e')
        .map(str::trim)
        .filter(|record| !record.is_empty())
        .map(|record| {
            let mut fields = record.split('\x1f');
            Commit {
                sha: fields.next().unwrap_or("").chars().take(9).collect(),
                subject: fields.next().unwrap_or("").to_string(),
                body: fields.next().unwrap_or("").to_string(),
                date: fields.next().unwrap_or("").to_string(),
                iso: fields.next().unwrap_or("").to_string(),
            }
        })
        .collect()
}

pub fn parse_file_commit_map(raw: &str) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    for record in raw.split('\x1e').map(str::trim).filter(|s| !s.is_empty()) {
        let mut lines = record.split('\n');
        let short = lines
            .next()
            .unwrap_or("")
            .chars()
            .take(9)
            .collect::<String>();

        for path in lines.map(str::trim).filter(|path| !path.is_empty()) {
            map.entry(path.to_string()).or_default().push(short.clone());
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_commit_log_reads_records_and_shortens_sha_to_nine_chars() {
        let raw = concat!(
            "123456789abcdef\x1fadd renderer\x1fbody text\nmore body\x1f2026-06-08 13:45\x1f2026-06-08T13:45:00-03:00\x1e",
            "abcdef123456789\x1ffix parser\x1f\x1f2026-06-09 09:10\x1f2026-06-09T09:10:00-03:00\x1e",
        );

        let commits = parse_commit_log(raw);

        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].sha, "123456789");
        assert_eq!(commits[0].subject, "add renderer");
        assert_eq!(commits[0].body, "body text\nmore body");
        assert_eq!(commits[0].date, "2026-06-08 13:45");
        assert_eq!(commits[0].iso, "2026-06-08T13:45:00-03:00");
        assert_eq!(commits[1].sha, "abcdef123");
        assert_eq!(commits[1].subject, "fix parser");
        assert_eq!(commits[1].body, "");
    }

    #[test]
    fn parse_commit_log_trims_blank_records_and_defaults_missing_fields() {
        let commits = parse_commit_log(" \n\x1e9876543210fedcb\x1fsubject only\x1e");

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].sha, "987654321");
        assert_eq!(commits[0].subject, "subject only");
        assert_eq!(commits[0].body, "");
        assert_eq!(commits[0].date, "");
        assert_eq!(commits[0].iso, "");
    }

    #[test]
    fn parse_file_commit_map_maps_paths_to_short_shas_in_record_order() {
        let raw = concat!(
            "\x1e123456789abcdef\nsrc/lib.rs\nsrc/git.rs\n\n",
            "\x1eabcdef123456789\nsrc/git.rs\nREADME.md\n",
        );

        let map = parse_file_commit_map(raw);

        assert_eq!(
            map.get("src/git.rs"),
            Some(&vec!["123456789".to_string(), "abcdef123".to_string()])
        );
        assert_eq!(map.get("src/lib.rs"), Some(&vec!["123456789".to_string()]));
        assert_eq!(map.get("README.md"), Some(&vec!["abcdef123".to_string()]));
    }

    #[test]
    fn parse_file_commit_map_ignores_blank_records_and_blank_paths() {
        let map = parse_file_commit_map("\x1e\n\x1e111111111222222\n\n  \npath.txt\n");

        assert_eq!(map.len(), 1);
        assert_eq!(map.get("path.txt"), Some(&vec!["111111111".to_string()]));
    }

    #[test]
    fn no_upstream_error_preserves_js_literal_branch_placeholder() {
        assert_eq!(
            no_upstream_error("fatal: no upstream configured for branch 'main'").to_string(),
            "fatal: no upstream configured for branch 'main'\nno upstream tracking branch (run: git push -u origin <branch>)"
        );
    }

    #[test]
    fn legacy_error_shape_keeps_git_stderr_before_script_message() {
        let error = legacy_script_error("fatal: Needed a single revision", "not a commit: nope");

        assert_eq!(
            error.to_string(),
            "fatal: Needed a single revision\nnot a commit: nope"
        );
    }
}
