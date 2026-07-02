use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::{
    attribution::{self, NewSide},
    git,
    model::{Commit, FileDiff, FileStatus, LineOwners},
};

pub fn parse_diff(raw: &str) -> Vec<FileDiff> {
    if raw.trim().is_empty() {
        return Vec::new();
    }

    let mut files = Vec::new();
    let mut cur: Option<FileDiff> = None;

    for line in raw.split('\n') {
        if let Some(rest) = line.strip_prefix("diff --git a/")
            && let Some((_, path)) = rest.split_once(" b/")
        {
            if let Some(file) = cur.take() {
                files.push(file);
            }
            cur = Some(FileDiff {
                path: path.to_string(),
                added: 0,
                removed: 0,
                lines: Vec::new(),
                full_lines: None,
                commits: Vec::new(),
                owners: LineOwners::default(),
            });
            continue;
        }

        let Some(file) = cur.as_mut() else {
            continue;
        };

        file.lines.push(line.to_string());
        if line.starts_with('+') && !line.starts_with("+++") {
            file.added += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            file.removed += 1;
        }
    }

    if let Some(file) = cur {
        files.push(file);
    }

    files
}

pub fn attach_full_context(files: &mut [FileDiff], full_files: Vec<FileDiff>) {
    let mut full_by_path: HashMap<String, Vec<String>> = full_files
        .into_iter()
        .map(|file| (file.path, file.lines))
        .collect();

    for file in files {
        if file.status() != FileStatus::Modified {
            continue;
        }

        let Some(full_lines) = full_by_path.remove(&file.path) else {
            continue;
        };

        if full_lines != file.lines {
            file.full_lines = Some(full_lines);
        }
    }
}

/// The assembled diff data for one preview: commits in range and changed files
/// (with per-line owners attached).
pub struct DiffData {
    pub commits: Vec<Commit>,
    pub files: Vec<FileDiff>,
}

/// The shared diff generator: log + diff + full-context + file-commit map +
/// per-line attribution. Takes range primitives so it stays decoupled from
/// `commands::Ranges`. Does NOT sort files — callers order as they always have.
pub fn assemble(
    repo: impl AsRef<Path>,
    diff_args: &[String],
    diff_range: &str,
    log_range: &str,
) -> anyhow::Result<DiffData> {
    let repo = repo.as_ref();
    let mut commits = git::log_commits(repo, log_range)?;
    let (base, new_side) = blame_targets(diff_range, log_range);

    // ! Blame never attributes a line to a merge, so a merge card is otherwise dead. Map each
    // ! merge to the commits it brought into the range so focusing it lifts their rows.
    for commit in commits.iter_mut() {
        if commit.is_merge() {
            commit.members = git::merge_members(repo, &commit.sha, &base).unwrap_or_default();
        }
    }

    let mut files = parse_diff(&git::diff_raw(repo, diff_args)?);
    attach_full_context(
        &mut files,
        parse_diff(&git::diff_raw(repo, &full_context_args(diff_args))?),
    );
    let file_commits = git::file_commit_map(repo, log_range)?;
    git::attach_commits(&mut files, &file_commits);

    let in_range: HashSet<String> = commits.iter().map(|commit| commit.sha.clone()).collect();
    attribution::attribute(repo, &base, &new_side, &in_range, &mut files);

    Ok(DiffData { commits, files })
}

// ! log_range is always two-dot `base..tip`; diff_range lacking `..` (hash mode) means the
// ! new side is the working tree, not a commit.
fn blame_targets(diff_range: &str, log_range: &str) -> (String, NewSide) {
    let base = log_range
        .split("..")
        .next()
        .unwrap_or(log_range)
        .to_string();
    let tip = log_range.rsplit("..").next().unwrap_or("HEAD").to_string();
    let new_side = if diff_range.contains("..") {
        NewSide::Commit(tip)
    } else {
        NewSide::WorkTree
    };
    (base, new_side)
}

pub fn full_context_args(args: &[String]) -> Vec<String> {
    let mut full_args = Vec::with_capacity(args.len() + 1);
    if let Some((cmd, rest)) = args.split_first() {
        full_args.push(cmd.clone());
        full_args.push("--unified=2147483647".to_string());
        full_args.extend(rest.iter().cloned());
    }
    full_args
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,2 +1,3 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n\
diff --git a/g.txt b/g.txt\n\
new file mode 100644\n\
index 000..333\n\
--- /dev/null\n\
+++ b/g.txt\n\
@@ -0,0 +1 @@\n\
+brand new\n";

    #[test]
    fn parse_diff_splits_two_file_diff_and_counts_body_changes() {
        let files = parse_diff(SAMPLE);

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "f.txt");
        assert_eq!(files[0].added, 2);
        assert_eq!(files[0].removed, 1);
        assert_eq!(files[1].path, "g.txt");
        assert_eq!(files[1].added, 1);
        assert_eq!(files[1].removed, 0);
    }

    #[test]
    fn parse_diff_does_not_count_file_headers_as_changes() {
        let files = parse_diff("diff --git a/a b/a\n--- a/a\n+++ b/a\n");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].added, 0);
        assert_eq!(files[0].removed, 0);
    }

    #[test]
    fn parse_diff_returns_empty_for_blank_input() {
        assert!(parse_diff("").is_empty());
        assert!(parse_diff("   \n\t").is_empty());
    }

    #[test]
    fn attach_full_context_only_sets_modified_files_with_extra_context() {
        let mut files = parse_diff(SAMPLE);
        let full = parse_diff(
            "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,4 +1,5 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n\
 middle\n\
 end\n\
diff --git a/g.txt b/g.txt\n\
new file mode 100644\n\
index 000..333\n\
--- /dev/null\n\
+++ b/g.txt\n\
@@ -0,0 +1 @@\n\
+brand new\n",
        );

        attach_full_context(&mut files, full);

        assert!(
            files[0]
                .full_lines
                .as_ref()
                .is_some_and(|lines| { lines.iter().any(|line| line == "middle") })
        );
        assert!(files[1].full_lines.is_none());
    }

    #[test]
    fn blame_targets_picks_base_tip_and_new_side() {
        let (base, side) = blame_targets("origin/main..HEAD", "origin/main..HEAD");
        assert_eq!(base, "origin/main");
        assert!(matches!(side, NewSide::Commit(ref tip) if tip == "HEAD"));

        let (base, side) = blame_targets("main...HEAD", "main..HEAD"); // merge mode
        assert_eq!(base, "main");
        assert!(matches!(side, NewSide::Commit(ref tip) if tip == "HEAD"));

        let (base, side) = blame_targets("abc123", "abc123..HEAD"); // hash mode -> worktree
        assert_eq!(base, "abc123");
        assert!(matches!(side, NewSide::WorkTree));

        let (base, side) = blame_targets("a1..b2", "a1..b2"); // exact range
        assert_eq!(base, "a1");
        assert!(matches!(side, NewSide::Commit(ref tip) if tip == "b2"));
    }

    #[test]
    fn full_context_args_inserts_unified_context_after_diff_command() {
        let args = vec!["diff".to_string(), "main..HEAD".to_string()];

        assert_eq!(
            full_context_args(&args),
            vec!["diff", "--unified=2147483647", "main..HEAD"]
        );
    }

    #[test]
    fn assemble_attaches_brought_in_members_to_a_merge() {
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
        g(&["branch", "-M", "main"]);
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

        let data = assemble(
            d,
            &["diff".to_string(), "main...HEAD".to_string()],
            "main...HEAD",
            "main..HEAD",
        )
        .unwrap();

        let merge = data
            .commits
            .iter()
            .find(|c| c.is_merge())
            .expect("a merge commit");
        let sub_b = data.commits.iter().find(|c| c.subject == "sub b").unwrap();
        let feat_a = data.commits.iter().find(|c| c.subject == "feat a").unwrap();

        assert!(
            merge.members.contains(&sub_b.sha),
            "merge lists its brought-in commit"
        );
        assert!(
            !merge.members.contains(&feat_a.sha),
            "first-parent commit is not a member"
        );
        assert!(
            feat_a.members.is_empty(),
            "a non-merge commit has no members"
        );
    }
}
