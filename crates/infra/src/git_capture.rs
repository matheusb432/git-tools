use std::{collections::HashMap, path::Path, process::Command};

use anyhow::{Context, anyhow};
use domain::diffs::Commit;

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
            "--format=%H%x1f%s%x1f%b%x1f%ad%x1f%aI%x1f%P%x1e",
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

// ! A merge is dead under blame (no line is attributed to it). Map it to the commits it
// ! brought into the previewed range: reachable from the merge, not from its first parent,
// ! and not from the base. `^<base>` prunes the walk to the range (a merge of `main` into
// ! the branch returns nothing — it introduces nothing to the preview).
pub fn merge_members(
    repo: impl AsRef<Path>,
    merge: &str,
    base: &str,
) -> anyhow::Result<Vec<String>> {
    let exclude_parent = format!("^{merge}^1");
    let exclude_base = format!("^{base}");
    let raw = run_git(repo, &["rev-list", merge, &exclude_parent, &exclude_base])?;
    Ok(parse_rev_list(&raw, merge))
}

/// Abbreviate a sha to the 9-char width this tool uses everywhere (matches the
/// `--short=9` git invocations); tolerates already-short input.
fn short_sha(sha: &str) -> String {
    sha.chars().take(9).collect()
}

fn parse_rev_list(raw: &str, merge: &str) -> Vec<String> {
    let merge_short = short_sha(merge);
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(short_sha)
        .filter(|sha| *sha != merge_short)
        .collect()
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
        &[
            "blame",
            "--porcelain",
            &format!("{base}..{tip}"),
            "--",
            path,
        ],
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

/// The repo's oldest root-commit sha (lexicographically smallest when several
/// roots exist), or `None` for a repo with no commits. Stable repo identity.
pub fn root_commit(repo: impl AsRef<Path>) -> Option<String> {
    let out = run_git(repo, &["rev-list", "--max-parents=0", "HEAD"]).ok()?;
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .min()
        .map(str::to_string)
}

/// Resolve a revision to its full 40-char sha.
pub fn resolve_sha(repo: impl AsRef<Path>, rev: &str) -> anyhow::Result<String> {
    Ok(run_git(repo, &["rev-parse", rev])?.trim().to_string())
}

/// The merge base of `a` and `b` as a full sha.
pub fn merge_base(repo: impl AsRef<Path>, a: &str, b: &str) -> anyhow::Result<String> {
    Ok(run_git(repo, &["merge-base", a, b])?.trim().to_string())
}

/// The committer date of `rev` as a strict ISO-8601 string (empty on failure).
pub fn committed_at(repo: impl AsRef<Path>, rev: &str) -> String {
    run_git(repo, &["show", "-s", "--format=%cI", rev])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
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
                sha: short_sha(fields.next().unwrap_or("")),
                subject: fields.next().unwrap_or("").to_string(),
                body: fields.next().unwrap_or("").to_string(),
                date: fields.next().unwrap_or("").to_string(),
                iso: fields.next().unwrap_or("").to_string(),
                parents: fields
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .map(short_sha)
                    .collect(),
                members: Vec::new(),
            }
        })
        .collect()
}

pub fn parse_file_commit_map(raw: &str) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    for record in raw.split('\x1e').map(str::trim).filter(|s| !s.is_empty()) {
        let mut lines = record.split('\n');
        let short = short_sha(lines.next().unwrap_or(""));

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
    fn parse_commit_log_reads_parents_and_flags_merge() {
        // record fields: sha · subject · body · date · iso · parents(space-sep)
        let raw = concat!(
            "merge12345678\x1fMerge branch 'sub'\x1f\x1f2026-06-08 13:45\x1f2026-06-08T13:45:00-03:00\x1faaaaaaaaa111 bbbbbbbbb222\x1e",
            "plain98765432\x1ffeat: x\x1f\x1f2026-06-09 09:10\x1f2026-06-09T09:10:00-03:00\x1faaaaaaaaa111\x1e",
        );

        let commits = parse_commit_log(raw);

        assert_eq!(
            commits[0].parents,
            vec!["aaaaaaaaa".to_string(), "bbbbbbbbb".to_string()]
        );
        assert!(commits[0].is_merge());
        assert_eq!(commits[1].parents, vec!["aaaaaaaaa".to_string()]);
        assert!(!commits[1].is_merge());
    }

    #[test]
    fn parse_rev_list_drops_the_merge_sha_and_shortens() {
        // rev-list lists the merge first, then its brought-in commits (full shas)
        let raw = "3c1a73a5acf40d58\n9386250ddffff00\nb74d1fcfeaaaa11\n";
        let members = parse_rev_list(raw, "3c1a73a5a");
        assert_eq!(
            members,
            vec!["9386250dd".to_string(), "b74d1fcfe".to_string()]
        );
    }

    #[test]
    fn merge_members_returns_in_range_brought_in_commits_only() {
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
                    .success(),
                "git {args:?} failed"
            );
        };
        g(&["init", "-q"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        std::fs::write(d.join("base.txt"), "base\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "base"]);
        g(&["branch", "-M", "main"]); // base ref = main, cross-version safe
        g(&["checkout", "-q", "-b", "feature"]);
        std::fs::write(d.join("a.txt"), "a\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "feat a"]);
        g(&["checkout", "-q", "-b", "sub"]);
        std::fs::write(d.join("b.txt"), "b\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "sub b"]);
        g(&["checkout", "-q", "feature"]);
        g(&["merge", "-q", "--no-ff", "sub", "-m", "Merge branch 'sub'"]);

        let merge = run_git(d, &["rev-parse", "--short=9", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        let sub_b = run_git(d, &["rev-parse", "--short=9", "HEAD^2"])
            .unwrap()
            .trim()
            .to_string();

        // brought into main..HEAD by the merge = sub b only (feat a is the first parent)
        assert_eq!(merge_members(d, &merge, "main").unwrap(), vec![sub_b]);

        // merging main INTO feature is base-reachable -> zero members within range
        g(&["checkout", "-q", "main"]);
        std::fs::write(d.join("m.txt"), "m\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "main moves"]);
        g(&["checkout", "-q", "feature"]);
        g(&[
            "merge",
            "-q",
            "--no-ff",
            "main",
            "-m",
            "Merge branch 'main'",
        ]);
        let merge_of_main = run_git(d, &["rev-parse", "--short=9", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert!(merge_members(d, &merge_of_main, "main").unwrap().is_empty());
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
        assert_eq!(root.len(), 40);
        let head = resolve_sha(d.to_str().unwrap(), "HEAD").unwrap();
        assert_eq!(root, head); // single commit ⇒ root == HEAD
    }
}
