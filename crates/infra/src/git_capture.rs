use std::{collections::HashMap, path::Path};

use anyhow::anyhow;
use application::ports::{BlameLines, GitDiffFormat, GitDiffRequest};
use domain::diffs::Commit;

pub(crate) fn run_git(repo: impl AsRef<Path>, args: &[&str]) -> anyhow::Result<String> {
    let output = crate::git_process::run(repo.as_ref(), args)?;
    if !output.success() {
        let stderr = output.stderr.trim().to_string();
        if stderr.is_empty() {
            return Err(anyhow!("git exited with {}", output.exit_code));
        }
        return Err(anyhow!(stderr));
    }

    Ok(output.stdout)
}

pub(crate) fn log_commits(repo: impl AsRef<Path>, range: &str) -> anyhow::Result<Vec<Commit>> {
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

pub(crate) fn file_commit_map(
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
pub(crate) fn merge_members(
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

pub(crate) fn diff(repo: impl AsRef<Path>, request: &GitDiffRequest) -> anyhow::Result<String> {
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
    run_git(repo, &args)
}

// ! Range-bounded forward blame of the tip: new-side line -> last commit that touched it.
pub(crate) fn blame_forward(
    repo: impl AsRef<Path>,
    base: &str,
    tip: &str,
    path: &str,
) -> anyhow::Result<BlameLines> {
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
    .map(|raw| parse_forward_blame(&raw))
}

// ! Working-tree blame for hash mode (diff is base -> worktree): aligns with worktree
// ! line numbers; uncommitted lines come back as the all-zero sha (out of range).
pub(crate) fn blame_forward_worktree(
    repo: impl AsRef<Path>,
    path: &str,
) -> anyhow::Result<BlameLines> {
    run_git(repo, &["blame", "--porcelain", "--", path]).map(|raw| parse_forward_blame(&raw))
}

// ! Reverse blame over the range: each deleted base line carries `previous <sha>` = its deleter.
pub(crate) fn blame_reverse(
    repo: impl AsRef<Path>,
    base: &str,
    tip: &str,
    path: &str,
) -> anyhow::Result<BlameLines> {
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
    .map(|raw| parse_reverse_blame(&raw))
}

fn blame_header(line: &str) -> Option<(String, u32)> {
    let mut fields = line.split(' ');
    let sha = fields.next()?;
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let _original_line = fields.next()?;
    let final_line = fields.next()?.parse().ok()?;
    Some((short_sha(sha), final_line))
}

fn parse_forward_blame(raw: &str) -> BlameLines {
    raw.lines()
        .filter_map(blame_header)
        .map(|(sha, line)| (line, sha))
        .collect()
}

fn parse_reverse_blame(raw: &str) -> BlameLines {
    let mut lines = HashMap::new();
    let mut deleter_by_sha = HashMap::new();
    let mut current_sha = None;
    let mut current_line = None;
    for line in raw.lines() {
        if let Some((sha, line_number)) = blame_header(line) {
            current_sha = Some(sha);
            current_line = Some(line_number);
        } else if let Some(rest) = line.strip_prefix("previous ")
            && let Some(sha) = current_sha.as_ref()
            && let Some(deleter) = rest.split(' ').next()
        {
            deleter_by_sha.insert(sha.clone(), short_sha(deleter));
        } else if line.starts_with('\t')
            && let (Some(sha), Some(line_number)) = (current_sha.as_ref(), current_line)
            && let Some(deleter) = deleter_by_sha.get(sha)
        {
            lines.insert(line_number, deleter.clone());
        }
    }
    lines
}

/// The repo's oldest root-commit sha (lexicographically smallest when several
/// roots exist), or `None` for a repo with no commits. Stable repo identity.
pub(crate) fn root_commit(repo: impl AsRef<Path>) -> Option<String> {
    let out = run_git(repo, &["rev-list", "--max-parents=0", "HEAD"]).ok()?;
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .min()
        .map(str::to_string)
}

/// The merge base of `a` and `b` as a full sha.
pub(crate) fn merge_base(repo: impl AsRef<Path>, a: &str, b: &str) -> anyhow::Result<String> {
    Ok(run_git(repo, &["merge-base", a, b])?.trim().to_string())
}

/// The committer date of `rev` as a strict ISO-8601 string (empty on failure).
pub(crate) fn committed_at(repo: impl AsRef<Path>, rev: &str) -> String {
    run_git(repo, &["show", "-s", "--format=%cI", rev])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub(crate) fn parse_commit_log(raw: &str) -> Vec<Commit> {
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

pub(crate) fn parse_file_commit_map(raw: &str) -> HashMap<String, Vec<String>> {
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
        let head = run_git(d, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(root, head); // single commit ⇒ root == HEAD
    }
}
