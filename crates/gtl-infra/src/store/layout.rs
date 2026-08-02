//! On-disk store layout: `<root>/<datetime>-<content-hash>.{html,json}`.
//! Writes are temp-file + atomic rename, so concurrent runs never corrupt state.
use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::Context;
use gtl_models::viewer::RenderOptions;

use crate::store::{
    id::content_hash,
    meta::{DiffKind, RENDERER_VERSION, Sidecar},
};

/// Result of placing an artifact: where it landed and whether it already existed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub path: PathBuf,
    pub reused: bool,
}

struct StoredArtifact {
    content_hash: String,
    html_path: PathBuf,
    sidecar: Sidecar,
}

/// Write `html` and its sidecar directly under the store root.
/// Idempotent: if the artifact already exists, nothing is written.
pub fn place(
    store_root: &Path,
    repo_id: &str,
    html: &str,
    sidecar: &Sidecar,
) -> anyhow::Result<Placed> {
    ensure_gitignore(store_root)?;
    let hash = content_hash(html);
    if let Some((html_path, json_path)) = existing_pair(store_root, repo_id, &hash) {
        let stored = fs::read_to_string(&json_path)
            .ok()
            .and_then(|json| serde_json::from_str::<Sidecar>(&json).ok());
        let needs_upgrade = stored.is_none_or(|stored| {
            !stored.theme_recorded || stored.renderer_version != RENDERER_VERSION
        });
        if needs_upgrade {
            atomic_write(
                &json_path,
                serde_json::to_string_pretty(sidecar)?.as_bytes(),
            )?;
        }
        return Ok(Placed {
            path: html_path,
            reused: true,
        });
    }
    let stem = format!(
        "{}-{hash}",
        filename_datetime(sidecar.generated_at.as_str())
    );
    let html_path = store_root.join(format!("{stem}.html"));
    let json_path = store_root.join(format!("{stem}.json"));
    atomic_write(&html_path, html.as_bytes())?;
    atomic_write(
        &json_path,
        serde_json::to_string_pretty(sidecar)?.as_bytes(),
    )?;
    Ok(Placed {
        path: html_path,
        reused: false,
    })
}

fn filename_datetime(generated_at: &str) -> String {
    let datetime: String = generated_at
        .trim()
        .chars()
        .map(|character| match character {
            '0'..='9' | 'A'..='Z' | 'a'..='z' | '-' => character,
            _ => '-',
        })
        .collect();
    if datetime.is_empty() {
        "unknown-time".to_string()
    } else {
        datetime
    }
}

fn existing_pair(
    store_root: &Path,
    repo_id: &str,
    content_hash: &str,
) -> Option<(PathBuf, PathBuf)> {
    let mut candidates: Vec<(PathBuf, PathBuf)> = fs::read_dir(store_root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "html")
        })
        .filter(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(content_hash_from_stem)
                .is_some_and(|candidate| candidate == content_hash)
        })
        .filter_map(|html_path| {
            let json_path = html_path.with_extension("json");
            if !json_path.exists() {
                return None;
            }
            let matches_repo = fs::read_to_string(&json_path)
                .ok()
                .and_then(|json| serde_json::from_str::<Sidecar>(&json).ok())
                .is_none_or(|stored| stored.repo_id == repo_id);
            matches_repo.then_some((html_path, json_path))
        })
        .collect();
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    candidates.into_iter().next()
}

fn ensure_gitignore(store_root: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(store_root).with_context(|| format!("create {}", store_root.display()))?;
    let path = store_root.join(".gitignore");
    let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("create {}", path.display()));
        }
    };
    file.write_all(b"*\n")
        .with_context(|| format!("write {}", path.display()))
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
    fs::rename(&tmp, final_path)
        .with_context(|| format!("rename into {}", final_path.display()))?;
    Ok(())
}

/// Find an existing artifact for a pure commit range rendered under the same
/// layout, density, theme, and exclusion set. Returns `None` for `WorkTree`
/// (never range-addressable) or on a miss. Scans the flat store's sidecars.
#[allow(
    clippy::too_many_arguments,
    reason = "the explicit fields are the persisted range-reuse key"
)]
pub fn lookup_by_range(
    store_root: &Path,
    repo_id: &str,
    kind: DiffKind,
    base_sha: &str,
    head_sha: &str,
    render_options: RenderOptions,
    theme: Option<&str>,
    excluded_extensions: &[String],
) -> Option<PathBuf> {
    if kind == DiffKind::WorkTree {
        return None;
    }
    let layout = render_options.layout().to_string();
    let density = render_options.density().to_string();
    for artifact in read_sidecars_paired(store_root) {
        if artifact.sidecar.repo_id == repo_id
            && artifact.sidecar.kind == kind
            && artifact.sidecar.base_sha == base_sha
            && artifact.sidecar.head_sha == head_sha
            && artifact.sidecar.layout == layout
            && artifact.sidecar.density == density
            && artifact.sidecar.theme_recorded
            && artifact.sidecar.renderer_version == RENDERER_VERSION
            && artifact.sidecar.theme.as_deref() == theme
            && artifact.sidecar.excluded_extensions == excluded_extensions
        {
            return Some(artifact.html_path);
        }
    }
    None
}

/// All sidecars across all repos, for the viewer's history.
/// Each entry pairs the sidecar with its content hash.
pub fn list_history_with_hash(store_root: &Path) -> Vec<(String, Sidecar)> {
    read_sidecars_paired(store_root)
        .into_iter()
        .map(|artifact| (artifact.content_hash, artifact.sidecar))
        .collect()
}

fn read_sidecars_paired(store_root: &Path) -> Vec<StoredArtifact> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(store_root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Some(content_hash) = content_hash_from_stem(stem) else {
                continue;
            };
            let html_path = path.with_extension("html");
            if !html_path.exists() {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(sidecar) = serde_json::from_str::<Sidecar>(&text) {
                out.push(StoredArtifact {
                    content_hash: content_hash.to_string(),
                    html_path,
                    sidecar,
                });
            }
        }
    }
    out
}

fn content_hash_from_stem(stem: &str) -> Option<&str> {
    let (_, content_hash) = stem.rsplit_once('-')?;
    (content_hash.len() == 16
        && content_hash
            .chars()
            .all(|character| character.is_ascii_hexdigit()))
    .then_some(content_hash)
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::{DiffDensity, DiffLayout, RenderOptions};

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
            layout: RenderOptions::DEFAULT.layout().to_string(),
            density: RenderOptions::DEFAULT.density().to_string(),
            theme: None,
            theme_recorded: true,
            renderer_version: RENDERER_VERSION,
            excluded_extensions: Vec::new(),
        }
    }

    #[test]
    fn place_creates_a_companion_gitignore() {
        let tmp = tempfile::tempdir().unwrap();

        place(
            tmp.path(),
            "repo0000",
            "<html>x</html>",
            &sidecar(DiffKind::TwoDot, "a", "b"),
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(tmp.path().join(".gitignore")).unwrap(),
            "*\n"
        );
    }

    #[test]
    fn place_leaves_an_existing_gitignore_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(".gitignore"), "custom\n").unwrap();

        place(
            tmp.path(),
            "repo0000",
            "<html>x</html>",
            &sidecar(DiffKind::TwoDot, "a", "b"),
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(tmp.path().join(".gitignore")).unwrap(),
            "custom\n"
        );
    }

    #[test]
    fn place_writes_a_flat_datetime_named_pair_then_reuses_it() {
        let tmp = tempfile::tempdir().unwrap();
        let html = "<html>x</html>";
        let mut sc = sidecar(DiffKind::TwoDot, "a", "b");
        sc.generated_at = "2026-07-28T12:34:56Z".into();

        let first = place(tmp.path(), "repo0000", html, &sc).unwrap();

        assert!(!first.reused);
        assert_eq!(
            first.path,
            tmp.path()
                .join(format!("2026-07-28T12-34-56Z-{}.html", content_hash(html)))
        );
        assert!(first.path.with_extension("json").exists());
        assert!(!tmp.path().join("diffs").exists());

        sc.generated_at = "2026-07-28T13:00:00Z".into();
        let second = place(tmp.path(), "repo0000", html, &sc).unwrap();

        assert!(second.reused);
        assert_eq!(first.path, second.path);
    }

    #[test]
    fn place_upgrades_legacy_theme_metadata_when_html_is_reused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut legacy = sidecar(DiffKind::TwoDot, "a", "b");
        legacy.theme_recorded = false;
        let first = place(tmp.path(), "repo0000", "<html>x</html>", &legacy).unwrap();
        let mut current = legacy;
        current.theme = Some("dark".to_string());
        current.theme_recorded = true;

        let second = place(tmp.path(), "repo0000", "<html>x</html>", &current).unwrap();

        assert!(second.reused);
        assert_eq!(second.path, first.path);
        let stored: Sidecar =
            serde_json::from_str(&fs::read_to_string(first.path.with_extension("json")).unwrap())
                .unwrap();
        assert_eq!(stored.theme, Some("dark".to_string()));
        assert!(stored.theme_recorded);
    }

    #[test]
    fn place_upgrades_stale_renderer_version_when_html_is_reused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut legacy = sidecar(DiffKind::TwoDot, "a", "b");
        legacy.renderer_version = 0;
        let first = place(tmp.path(), "repo0000", "<html>x</html>", &legacy).unwrap();
        let current = sidecar(DiffKind::TwoDot, "a", "b");

        let second = place(tmp.path(), "repo0000", "<html>x</html>", &current).unwrap();

        assert!(second.reused);
        assert_eq!(second.path, first.path);
        let stored: Sidecar =
            serde_json::from_str(&fs::read_to_string(first.path.with_extension("json")).unwrap())
                .unwrap();
        assert_eq!(stored.renderer_version, RENDERER_VERSION);
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
        let hit = lookup_by_range(
            tmp.path(),
            "repo0000",
            DiffKind::TwoDot,
            "aaaa",
            "bbbb",
            RenderOptions::DEFAULT,
            None,
            &[],
        );
        assert!(hit.is_some());
        let miss = lookup_by_range(
            tmp.path(),
            "repo0000",
            DiffKind::TwoDot,
            "aaaa",
            "cccc",
            RenderOptions::DEFAULT,
            None,
            &[],
        );
        assert!(miss.is_none());
    }

    #[test]
    fn lookup_by_range_uses_sidecar_repo_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sc = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        sc.repo_id = "repoBBBB".into();
        place(tmp.path(), "repoBBBB", "<html>x</html>", &sc).unwrap();

        assert!(
            lookup_by_range(
                tmp.path(),
                "repoAAAA",
                DiffKind::TwoDot,
                "aaaa",
                "bbbb",
                RenderOptions::DEFAULT,
                None,
                &[],
            )
            .is_none()
        );
        assert!(
            lookup_by_range(
                tmp.path(),
                "repoBBBB",
                DiffKind::TwoDot,
                "aaaa",
                "bbbb",
                RenderOptions::DEFAULT,
                None,
                &[],
            )
            .is_some()
        );
    }

    #[test]
    fn lookup_by_range_requires_the_same_exclusion_set() {
        let tmp = tempfile::tempdir().unwrap();
        // An artifact rendered without exclusions (or by a pre-exclusion build,
        // whose sidecar lacks the field entirely) …
        let sc = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        place(tmp.path(), "repo0000", "<html>unfiltered</html>", &sc).unwrap();
        // … must never satisfy a render running under an active filter.
        let filtered = lookup_by_range(
            tmp.path(),
            "repo0000",
            DiffKind::TwoDot,
            "aaaa",
            "bbbb",
            RenderOptions::DEFAULT,
            None,
            &["md".to_string()],
        );
        assert!(filtered.is_none(), "stale unfiltered artifact was reused");

        // And a filtered artifact is only reusable under the identical set.
        let mut filtered_sidecar = sidecar(DiffKind::TwoDot, "cccc", "dddd");
        filtered_sidecar.excluded_extensions = vec!["md".to_string()];
        place(
            tmp.path(),
            "repo0000",
            "<html>filtered</html>",
            &filtered_sidecar,
        )
        .unwrap();
        assert!(
            lookup_by_range(
                tmp.path(),
                "repo0000",
                DiffKind::TwoDot,
                "cccc",
                "dddd",
                RenderOptions::DEFAULT,
                None,
                &["md".to_string()],
            )
            .is_some()
        );
        assert!(
            lookup_by_range(
                tmp.path(),
                "repo0000",
                DiffKind::TwoDot,
                "cccc",
                "dddd",
                RenderOptions::DEFAULT,
                None,
                &[]
            )
            .is_none(),
            "filtered artifact must not serve an unfiltered render"
        );
    }

    #[test]
    fn lookup_by_range_requires_the_same_recorded_theme() {
        let tmp = tempfile::tempdir().unwrap();
        let mut dark = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        dark.layout = DiffLayout::Split.to_string();
        dark.density = DiffDensity::Full.to_string();
        dark.theme = Some("dark".to_string());
        dark.excluded_extensions = vec!["md".to_string()];
        place(tmp.path(), "repo0000", "<html>dark</html>", &dark).unwrap();

        assert!(
            lookup_by_range(
                tmp.path(),
                "repo0000",
                DiffKind::TwoDot,
                "aaaa",
                "bbbb",
                RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
                Some("dark"),
                &["md".to_string()],
            )
            .is_some()
        );
        assert!(
            lookup_by_range(
                tmp.path(),
                "repo0000",
                DiffKind::TwoDot,
                "aaaa",
                "bbbb",
                RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
                Some("light"),
                &["md".to_string()],
            )
            .is_none()
        );

        let mut legacy = sidecar(DiffKind::TwoDot, "cccc", "dddd");
        legacy.theme_recorded = false;
        place(tmp.path(), "repo0000", "<html>legacy</html>", &legacy).unwrap();
        assert!(
            lookup_by_range(
                tmp.path(),
                "repo0000",
                DiffKind::TwoDot,
                "cccc",
                "dddd",
                RenderOptions::DEFAULT,
                None,
                &[],
            )
            .is_none(),
            "sidecars without theme metadata must not satisfy range reuse"
        );
    }

    #[test]
    fn lookup_by_range_requires_the_same_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let mut split = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        split.layout = DiffLayout::Split.to_string();
        place(tmp.path(), "repo0000", "<html>split</html>", &split).unwrap();

        let hit = lookup_by_range(
            tmp.path(),
            "repo0000",
            DiffKind::TwoDot,
            "aaaa",
            "bbbb",
            RenderOptions::new(DiffLayout::Split, DiffDensity::Compact),
            None,
            &[],
        );
        let miss = lookup_by_range(
            tmp.path(),
            "repo0000",
            DiffKind::TwoDot,
            "aaaa",
            "bbbb",
            RenderOptions::DEFAULT,
            None,
            &[],
        );

        assert!(hit.is_some());
        assert!(miss.is_none());
    }

    #[test]
    fn lookup_by_range_requires_the_same_density() {
        let tmp = tempfile::tempdir().unwrap();
        let mut full = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        full.density = DiffDensity::Full.to_string();
        place(tmp.path(), "repo0000", "<html>full</html>", &full).unwrap();

        let hit = lookup_by_range(
            tmp.path(),
            "repo0000",
            DiffKind::TwoDot,
            "aaaa",
            "bbbb",
            RenderOptions::new(DiffLayout::Unified, DiffDensity::Full),
            None,
            &[],
        );
        let miss = lookup_by_range(
            tmp.path(),
            "repo0000",
            DiffKind::TwoDot,
            "aaaa",
            "bbbb",
            RenderOptions::DEFAULT,
            None,
            &[],
        );

        assert!(hit.is_some());
        assert!(miss.is_none());
    }

    #[test]
    fn lookup_by_range_skips_sidecars_from_older_renderers() {
        let tmp = tempfile::tempdir().unwrap();
        let mut legacy = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        legacy.renderer_version = 0;
        place(tmp.path(), "repo0000", "<html>old</html>", &legacy).unwrap();

        let miss = lookup_by_range(
            tmp.path(),
            "repo0000",
            DiffKind::TwoDot,
            "aaaa",
            "bbbb",
            RenderOptions::DEFAULT,
            None,
            &[],
        );
        assert!(
            miss.is_none(),
            "legacy renderer artifact must not satisfy reuse"
        );

        let current = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        place(tmp.path(), "repo0000", "<html>new</html>", &current).unwrap();
        let hit = lookup_by_range(
            tmp.path(),
            "repo0000",
            DiffKind::TwoDot,
            "aaaa",
            "bbbb",
            RenderOptions::DEFAULT,
            None,
            &[],
        );
        assert!(hit.is_some(), "current renderer artifact must hit");
    }

    #[test]
    fn worktree_is_never_range_addressable() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            lookup_by_range(
                tmp.path(),
                "r",
                DiffKind::WorkTree,
                "a",
                "b",
                RenderOptions::DEFAULT,
                None,
                &[],
            )
            .is_none()
        );
    }

    #[test]
    fn list_history_with_hash_collects_across_repos() {
        let tmp = tempfile::tempdir().unwrap();
        place(
            tmp.path(),
            "repoAAAA",
            "<a/>",
            &sidecar(DiffKind::TwoDot, "a", "b"),
        )
        .unwrap();
        place(
            tmp.path(),
            "repoBBBB",
            "<b/>",
            &sidecar(DiffKind::ThreeDot, "c", "d"),
        )
        .unwrap();
        assert_eq!(list_history_with_hash(tmp.path()).len(), 2);
    }

    #[test]
    fn list_history_with_hash_pairs_content_hash_to_sidecar() {
        let tmp = tempfile::tempdir().unwrap();
        place(
            tmp.path(),
            "repoAAAA",
            "<a/>",
            &sidecar(DiffKind::TwoDot, "a", "b"),
        )
        .unwrap();
        let got = list_history_with_hash(tmp.path());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, content_hash("<a/>"));
    }
}
