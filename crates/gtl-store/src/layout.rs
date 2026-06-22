//! On-disk store layout: `<root>/diffs/<repo_id>/<content-hash>.{html,json}`.
//! Writes are temp-file + atomic rename, so concurrent runs never corrupt state.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;

use crate::id::content_hash;
use crate::meta::{DiffKind, Sidecar};

/// Result of placing an artifact: where it landed and whether it already existed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub path: PathBuf,
    pub reused: bool,
}

fn repo_dir(store_root: &Path, repo_id: &str) -> PathBuf {
    store_root.join("diffs").join(repo_id)
}

/// Write `html` + its `sidecar` under the repo dir, addressed by content hash.
/// Idempotent: if the artifact already exists, nothing is written.
pub fn place(
    store_root: &Path,
    repo_id: &str,
    html: &str,
    sidecar: &Sidecar,
) -> anyhow::Result<Placed> {
    let hash = content_hash(html);
    let dir = repo_dir(store_root, repo_id);
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let html_path = dir.join(format!("{hash}.html"));
    let json_path = dir.join(format!("{hash}.json"));
    // Reuse only when BOTH files exist; if the sidecar was lost, (re)write both.
    if html_path.exists() && json_path.exists() {
        return Ok(Placed { path: html_path, reused: true });
    }
    atomic_write(&html_path, html.as_bytes())?;
    atomic_write(&json_path, serde_json::to_string_pretty(sidecar)?.as_bytes())?;
    Ok(Placed { path: html_path, reused: false })
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn atomic_write(final_path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let file_name = final_path
        .file_name()
        .and_then(|n| n.to_str())
        .context("artifact path has no file name")?;
    let unique = format!(
        "{file_name}.{}.{}.tmp",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let tmp = final_path.with_file_name(unique);
    fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, final_path).with_context(|| format!("rename into {}", final_path.display()))?;
    Ok(())
}

/// Find an existing artifact for a pure commit range. Returns `None` for
/// `WorkTree` (never range-addressable) or on a miss. Scans the repo's sidecars.
pub fn lookup_by_range(
    store_root: &Path,
    repo_id: &str,
    kind: DiffKind,
    base_sha: &str,
    head_sha: &str,
) -> anyhow::Result<Option<PathBuf>> {
    if kind == DiffKind::WorkTree {
        return Ok(None);
    }
    let dir = repo_dir(store_root, repo_id);
    for (stem, sidecar) in read_sidecars_paired(&dir)? {
        if sidecar.kind == kind && sidecar.base_sha == base_sha && sidecar.head_sha == head_sha {
            return Ok(Some(dir.join(format!("{stem}.html"))));
        }
    }
    Ok(None)
}

/// All sidecars across all repos, for the viewer's history.
/// Each entry pairs the sidecar with its content hash (the filename stem), so
/// callers can build `diff://` URLs without re-reading the filesystem.
pub fn list_history_with_hash(store_root: &Path) -> anyhow::Result<Vec<(String, Sidecar)>> {
    let diffs = store_root.join("diffs");
    let mut all = Vec::new();
    let Ok(repos) = fs::read_dir(&diffs) else { return Ok(all) };
    for repo in repos.flatten() {
        all.extend(read_sidecars_paired(&repo.path())?);
    }
    Ok(all)
}

fn read_sidecars_paired(dir: &Path) -> anyhow::Result<Vec<(String, Sidecar)>> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return Ok(out) };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let Ok(text) = fs::read_to_string(&path) else { continue };
            if let Ok(sc) = serde_json::from_str::<Sidecar>(&text) {
                out.push((stem, sc));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sidecar(kind: DiffKind, base: &str, head: &str) -> Sidecar {
        Sidecar {
            repo_id: "repo0000".into(),
            repo_name: "r".into(),
            repo_root: "/r".into(),
            kind,
            base_sha: base.into(),
            head_sha: head.into(),
            range_label: "x".into(),
            head_committed_at: "t".into(),
            generated_at: "t".into(),
            title: "diff".into(),
            byte_size: 0,
        }
    }

    #[test]
    fn place_writes_then_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let sc = sidecar(DiffKind::TwoDot, "a", "b");
        let first = place(tmp.path(), "repo0000", "<html>x</html>", &sc).unwrap();
        assert!(!first.reused);
        assert!(first.path.exists());
        let second = place(tmp.path(), "repo0000", "<html>x</html>", &sc).unwrap();
        assert!(second.reused);
        assert_eq!(first.path, second.path);
    }

    #[test]
    fn place_repairs_when_sidecar_deleted_but_html_remains() {
        let tmp = tempfile::tempdir().unwrap();
        let sc = sidecar(DiffKind::TwoDot, "a", "b");
        let first = place(tmp.path(), "repo0000", "<html>repair</html>", &sc).unwrap();
        assert!(!first.reused);
        // Delete only the sidecar, leaving the .html behind.
        let json_path = first.path.with_extension("json");
        fs::remove_file(&json_path).unwrap();
        assert!(first.path.exists());
        assert!(!json_path.exists());
        // A subsequent place must rewrite both files, not report reused.
        let repaired = place(tmp.path(), "repo0000", "<html>repair</html>", &sc).unwrap();
        assert!(!repaired.reused, "should rewrite when sidecar is absent");
        assert!(json_path.exists(), "sidecar must be recreated");
    }

    #[test]
    fn lookup_by_range_finds_a_commit_range_hit() {
        let tmp = tempfile::tempdir().unwrap();
        let sc = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        place(tmp.path(), "repo0000", "<html>x</html>", &sc).unwrap();
        let hit = lookup_by_range(tmp.path(), "repo0000", DiffKind::TwoDot, "aaaa", "bbbb").unwrap();
        assert!(hit.is_some());
        let miss = lookup_by_range(tmp.path(), "repo0000", DiffKind::TwoDot, "aaaa", "cccc").unwrap();
        assert!(miss.is_none());
    }

    #[test]
    fn worktree_is_never_range_addressable() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(lookup_by_range(tmp.path(), "r", DiffKind::WorkTree, "a", "b").unwrap().is_none());
    }

    #[test]
    fn list_history_with_hash_collects_across_repos() {
        let tmp = tempfile::tempdir().unwrap();
        place(tmp.path(), "repoAAAA", "<a/>", &sidecar(DiffKind::TwoDot, "a", "b")).unwrap();
        place(tmp.path(), "repoBBBB", "<b/>", &sidecar(DiffKind::ThreeDot, "c", "d")).unwrap();
        assert_eq!(list_history_with_hash(tmp.path()).unwrap().len(), 2);
    }

    #[test]
    fn list_history_with_hash_pairs_stem_to_sidecar() {
        let tmp = tempfile::tempdir().unwrap();
        let placed = place(tmp.path(), "repoAAAA", "<a/>", &sidecar(DiffKind::TwoDot, "a", "b")).unwrap();
        let stem = placed.path.file_stem().unwrap().to_str().unwrap().to_string();
        let got = list_history_with_hash(tmp.path()).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, stem);
    }
}
