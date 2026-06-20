//! Per-line commit attribution for the diff preview.
//!
//! Each aggregate-diff row is mapped to the commit that owns it: forward
//! `git blame` for added rows (new-side line → author), reverse `git blame` for
//! deleted rows (old-side line → the `previous` commit that removed it). Only
//! shas inside the previewed range are kept, so an unattributable row is left
//! bare rather than mis-assigned.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::model::FileDiff;

/// The new (right) side of the previewed diff: a committed tip, or the working
/// tree (hash mode, where the diff is `base` → working tree).
pub enum NewSide {
    Commit(String),
    WorkTree,
}

/// Fill each file's `owners` from git blame, keeping only in-range shas. Blame
/// errors are swallowed per file (it goes unattributed) so one awkward file —
/// deleted, binary, renamed — never aborts the preview.
pub fn attribute(
    repo: impl AsRef<Path>,
    base: &str,
    new_side: &NewSide,
    in_range: &HashSet<String>,
    files: &mut [FileDiff],
) {
    let repo = repo.as_ref();
    let tip = match new_side {
        NewSide::Commit(tip) => tip.as_str(),
        NewSide::WorkTree => "HEAD",
    };
    for file in files.iter_mut() {
        let forward = match new_side {
            NewSide::Commit(tip) => crate::git::blame_forward(repo, base, tip, &file.path),
            NewSide::WorkTree => crate::git::blame_forward_worktree(repo, &file.path),
        };
        if let Ok(raw) = forward {
            file.owners.added = parse_forward(&raw, in_range);
        }
        if let Ok(raw) = crate::git::blame_reverse(repo, base, tip, &file.path) {
            file.owners.deleted = parse_reverse(&raw, in_range);
        }
    }
}

// ! Blame porcelain line header: "<40-hex sha> <orig> <final> [group-count]".
// ! `final` is the annotated-file line number (forward: new side; reverse: base side);
// ! a 4th token marks a group start whose metadata block follows.
fn header(line: &str) -> Option<(String, u32, bool)> {
    let mut it = line.split(' ');
    let sha = it.next()?;
    if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let _orig = it.next()?;
    let final_no: u32 = it.next()?.parse().ok()?;
    let has_count = it.next().is_some();
    Some((sha.chars().take(9).collect(), final_no, has_count))
}

fn parse_forward(raw: &str, in_range: &HashSet<String>) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    for line in raw.lines() {
        if let Some((sha, final_no, _)) = header(line)
            && in_range.contains(&sha)
        {
            map.insert(final_no, sha);
        }
    }
    map
}

fn parse_reverse(raw: &str, in_range: &HashSet<String>) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    // ! Porcelain emits a commit's metadata (incl. `previous`) ONCE, on its first group; later
    // ! groups of the same sha carry only an abbreviated header. So key the deleter by sha, not
    // ! by group. A base line attributed to a non-tip commit is, by reverse-blame definition,
    // ! deleted — its deleter is that commit's `previous`.
    let mut deleter_by_sha: HashMap<String, String> = HashMap::new();
    let mut cur_sha: Option<String> = None;
    let mut base_line: Option<u32> = None;
    for line in raw.lines() {
        if let Some((sha, final_no, _)) = header(line) {
            cur_sha = Some(sha);
            base_line = Some(final_no);
        } else if let Some(rest) = line.strip_prefix("previous ")
            && let Some(sha) = cur_sha.as_ref()
            && let Some(deleter) = rest.split(' ').next()
        {
            deleter_by_sha.insert(sha.clone(), deleter.chars().take(9).collect());
        } else if line.starts_with('\t')
            && let (Some(sha), Some(bl)) = (cur_sha.as_ref(), base_line)
            && let Some(deleter) = deleter_by_sha.get(sha)
            && in_range.contains(deleter)
        {
            map.insert(bl, deleter.clone());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::model::{FileDiff, LineOwners};
    use std::path::Path;
    use std::process::Command;

    fn set(shas: &[&str]) -> HashSet<String> {
        shas.iter().map(|s| s.to_string()).collect()
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?} failed");
    }

    fn short_head(dir: &Path) -> String {
        crate::git::run_git(dir, &["rev-parse", "--short=9", "HEAD"])
            .unwrap()
            .trim()
            .to_string()
    }

    #[test]
    fn attribute_owns_added_and_deleted_lines_by_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        git(d, &["init", "-q"]);
        git(d, &["config", "user.email", "t@t"]);
        git(d, &["config", "user.name", "t"]);
        std::fs::write(d.join("f.txt"), "L1\nL2\nL3\nL4\nL5\n").unwrap();
        git(d, &["add", "."]);
        git(d, &["commit", "-qm", "base"]);
        let base = short_head(d);
        // c1 inserts ADD at new line 3
        std::fs::write(d.join("f.txt"), "L1\nL2\nADD\nL3\nL4\nL5\n").unwrap();
        git(d, &["commit", "-qam", "c1"]);
        let c1 = short_head(d);
        // c2 deletes L4 (base line 4)
        std::fs::write(d.join("f.txt"), "L1\nL2\nADD\nL3\nL5\n").unwrap();
        git(d, &["commit", "-qam", "c2"]);
        let c2 = short_head(d);

        let mut files = vec![FileDiff {
            path: "f.txt".to_string(),
            added: 0,
            removed: 0,
            lines: Vec::new(),
            full_lines: None,
            commits: Vec::new(),
            owners: LineOwners::default(),
        }];
        let in_range = set(&[&c1, &c2]);
        attribute(d, &base, &NewSide::Commit("HEAD".to_string()), &in_range, &mut files);

        assert_eq!(files[0].owners.added.get(&3), Some(&c1)); // ADD at new line 3 -> c1
        assert_eq!(files[0].owners.deleted.get(&4), Some(&c2)); // base line 4 (L4) -> c2
    }

    // forward porcelain: header "<40hex> <orig> <final> [count]" then "\t<code>"
    const FWD: &str = "\
6ea844233dc9bc0454f940e6a20d2d4695c355f3 3 3 1
\tC1 added
9ccfcca0ba2274515cf8b1c671bdbc6556e36f7f 5 5 1
\tL5 mod
93b22953a80d171c0ba22ebb88f4583ab17d4df0 1 1 2
\tL1 base
93b22953a80d171c0ba22ebb88f4583ab17d4df0 2 2
\tL2 base
";

    #[test]
    fn parse_forward_maps_final_line_to_in_range_sha_only() {
        let m = parse_forward(FWD, &set(&["6ea844233", "9ccfcca0b"]));
        assert_eq!(m.get(&3), Some(&"6ea844233".to_string()));
        assert_eq!(m.get(&5), Some(&"9ccfcca0b".to_string()));
        // 93b22953a is the base commit, out of range -> filtered out
        assert_eq!(m.get(&1), None);
    }

    // reverse porcelain: header final = base line; "previous <deleter> <file>" per group
    const REV: &str = "\
6ea844233dc9bc0454f940e6a20d2d4695c355f3 5 4 1
previous a4345a5e16955afb108eaf9100dfa677f0d3190f f.txt
filename f.txt
\tL4 base
a4345a5e16955afb108eaf9100dfa677f0d3190f 5 5 1
previous 9ccfcca0ba2274515cf8b1c671bdbc6556e36f7f f.txt
filename f.txt
\tL5 base
";

    #[test]
    fn parse_reverse_maps_base_line_to_deleter_previous_sha() {
        let m = parse_reverse(REV, &set(&["a4345a5e1", "9ccfcca0b"]));
        assert_eq!(m.get(&4), Some(&"a4345a5e1".to_string())); // L4 deleted by c2
        assert_eq!(m.get(&5), Some(&"9ccfcca0b".to_string())); // L5 deleted by c3
    }

    #[test]
    fn parse_reverse_propagates_previous_across_grouped_deletion() {
        // two contiguous base lines (10,11) deleted by one commit: counted header + continuation
        let raw = "\
1111111111111111111111111111111111111111 10 10 2
previous 2222222222222222222222222222222222222222 f.txt
filename f.txt
\told a
1111111111111111111111111111111111111111 11 11
\told b
";
        let m = parse_reverse(raw, &set(&["222222222"]));
        assert_eq!(m.get(&10), Some(&"222222222".to_string()));
        assert_eq!(m.get(&11), Some(&"222222222".to_string()));
    }

    #[test]
    fn parse_reverse_reuses_previous_for_later_groups_of_same_sha() {
        // porcelain emits `previous` once per sha (first group); a later group of the same sha
        // (here base lines 27,28) is abbreviated yet must still resolve to the same deleter.
        let raw = "\
1111111111111111111111111111111111111111 4 4 1
previous 2222222222222222222222222222222222222222 f.txt
filename f.txt
\timport line
1111111111111111111111111111111111111111 27 27 2
\tblock line a
1111111111111111111111111111111111111111 28 28
\tblock line b
";
        let m = parse_reverse(raw, &set(&["222222222"]));
        assert_eq!(m.get(&4), Some(&"222222222".to_string()));
        assert_eq!(m.get(&27), Some(&"222222222".to_string()));
        assert_eq!(m.get(&28), Some(&"222222222".to_string()));
    }

    #[test]
    fn parse_reverse_skips_surviving_lines_without_previous() {
        let raw = "\
9999999999999999999999999999999999999999 1 1 1
filename f.txt
\tsurvives
";
        assert!(parse_reverse(raw, &set(&["999999999"])).is_empty());
    }
}
