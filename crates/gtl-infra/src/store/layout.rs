//! On-disk store layout: `<root>/<datetime>-<content-hash>.{html,json}`.
//! Writes are temp-file + atomic rename, so concurrent runs never corrupt state.
use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::Context;
use gtl_application::ports::{ArtifactRangeKey, PlacedArtifact};
use gtl_models::{
    artifacts::{ArtifactContentHash, RepositoryStoreId},
    paths::AbsoluteFilePath,
};

use crate::store::{
    id::content_hash,
    meta::{ArtifactMetadata, ArtifactThemeMetadata, RENDERER_VERSION, Sidecar},
};

struct StoredArtifact {
    content_hash: ArtifactContentHash,
    html_path: PathBuf,
    metadata: ArtifactMetadata,
}

/// Write `html` and its sidecar directly under the store root.
/// Idempotent: if the artifact already exists, nothing is written.
pub fn place(
    store_root: &Path,
    repo_id: &RepositoryStoreId,
    html: &str,
    metadata: &ArtifactMetadata,
) -> anyhow::Result<PlacedArtifact> {
    ensure_gitignore(store_root)?;
    let hash = content_hash(html);
    if let Some((html_path, json_path, stored)) = existing_pair(store_root, repo_id, &hash) {
        let needs_upgrade = matches!(stored.theme, ArtifactThemeMetadata::Unrecorded)
            || stored.renderer_version != RENDERER_VERSION;
        if needs_upgrade {
            atomic_write(
                &json_path,
                serde_json::to_string_pretty(&Sidecar::from_metadata(metadata))?.as_bytes(),
            )?;
        }
        return Ok(PlacedArtifact::Reused {
            path: AbsoluteFilePath::try_new(html_path)
                .context("stored artifact path is not absolute")?,
        });
    }
    let stem = format!("{}-{hash}", filename_datetime(&metadata.generated_at));
    let html_path = store_root.join(format!("{stem}.html"));
    let json_path = store_root.join(format!("{stem}.json"));
    atomic_write(&html_path, html.as_bytes())?;
    atomic_write(
        &json_path,
        serde_json::to_string_pretty(&Sidecar::from_metadata(metadata))?.as_bytes(),
    )?;
    Ok(PlacedArtifact::Created {
        path: AbsoluteFilePath::try_new(html_path).context("artifact path is not absolute")?,
    })
}

fn filename_datetime(generated_at: &gtl_models::timestamps::MachineTimestamp) -> String {
    let datetime: String = generated_at
        .as_ref()
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
    repo_id: &RepositoryStoreId,
    content_hash: &ArtifactContentHash,
) -> Option<(PathBuf, PathBuf, ArtifactMetadata)> {
    let mut candidates: Vec<(PathBuf, PathBuf, ArtifactMetadata)> = fs::read_dir(store_root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "html")
        })
        .filter_map(|html_path| {
            let candidate = html_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(content_hash_from_stem)?;
            if &candidate != content_hash {
                return None;
            }
            let json_path = html_path.with_extension("json");
            if !json_path.exists() {
                return None;
            }
            let metadata = fs::read_to_string(&json_path)
                .ok()
                .and_then(|json| serde_json::from_str::<Sidecar>(&json).ok())
                .and_then(|sidecar| sidecar.try_into_metadata().ok())?;
            (metadata.repo_id == *repo_id).then_some((html_path, json_path, metadata))
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
/// layout, density, theme, and exclusion set. Returns `None` on a miss and
/// scans the flat store's validated sidecar projections.
pub fn lookup_by_range(
    store_root: &Path,
    repo_id: &RepositoryStoreId,
    key: &ArtifactRangeKey,
) -> Option<AbsoluteFilePath> {
    for artifact in read_sidecars_paired(store_root) {
        if artifact.metadata.repo_id == *repo_id
            && artifact.metadata.identity.matches_commit_range(&key.range)
            && artifact.metadata.render_options == key.render_options
            && artifact.metadata.theme == ArtifactThemeMetadata::Recorded(key.theme)
            && artifact.metadata.renderer_version == RENDERER_VERSION
            && artifact.metadata.excluded_extensions == key.excluded_extensions
        {
            return AbsoluteFilePath::try_new(artifact.html_path).ok();
        }
    }
    None
}

/// All sidecars across all repos, for the viewer's history.
/// Each entry pairs the sidecar with its content hash.
pub fn list_history_with_hash(store_root: &Path) -> Vec<(ArtifactContentHash, ArtifactMetadata)> {
    read_sidecars_paired(store_root)
        .into_iter()
        .map(|artifact| (artifact.content_hash, artifact.metadata))
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
            let Ok(sidecar) = serde_json::from_str::<Sidecar>(&text) else {
                continue;
            };
            let Ok(metadata) = sidecar.try_into_metadata() else {
                continue;
            };
            out.push(StoredArtifact {
                content_hash,
                html_path,
                metadata,
            });
        }
    }
    out
}

fn content_hash_from_stem(stem: &str) -> Option<ArtifactContentHash> {
    let (_, content_hash) = stem.rsplit_once('-')?;
    ArtifactContentHash::try_new(content_hash.to_owned()).ok()
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        artifacts::{ArtifactCommitRange, ArtifactRangeKind},
        diffs::{DiffKind, ExcludedExtensions},
        viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
    };

    use super::*;
    use crate::testing::pinned_range;

    fn repository_id(label: &str) -> RepositoryStoreId {
        crate::store::repo_id(None, Path::new(label))
    }

    fn place(
        store_root: &Path,
        repository_label: &str,
        html: &str,
        sidecar: &Sidecar,
    ) -> anyhow::Result<PlacedArtifact> {
        super::place(
            store_root,
            &repository_id(repository_label),
            html,
            &sidecar.clone().try_into_metadata()?,
        )
    }

    fn lookup_by_range(
        store_root: &Path,
        repository_label: &str,
        key: &ArtifactRangeKey,
    ) -> Option<AbsoluteFilePath> {
        super::lookup_by_range(store_root, &repository_id(repository_label), key)
    }

    fn sidecar(kind: DiffKind, base: &str, head: &str) -> Sidecar {
        Sidecar {
            repo_id: repository_id("repo0000").to_string(),
            repo_name: "r".into(),
            repo_root: "/r".into(),
            kind,
            base_sha: commit_id_text(base),
            head_sha: commit_id_text(head),
            range_label: "x".into(),
            head_committed_at: "2026-07-03T00:00:00Z".into(),
            generated_at: "2026-07-03T00:01:00Z".into(),
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

    fn range_key(kind: DiffKind, base_sha: &str, head_sha: &str) -> ArtifactRangeKey {
        ArtifactRangeKey {
            range: ArtifactCommitRange {
                kind: ArtifactRangeKind::try_from(kind).expect("fixture kind is range-addressable"),
                commits: pinned_range(base_sha, head_sha),
            },
            render_options: RenderOptions::DEFAULT,
            theme: None,
            excluded_extensions: ExcludedExtensions::default(),
        }
    }

    fn commit_id_text(prefix: &str) -> String {
        prefix.chars().cycle().take(40).collect()
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

        assert!(!first.is_reused());
        assert_eq!(
            first.path().as_path(),
            tmp.path()
                .join(format!("2026-07-28T12-34-56Z-{}.html", content_hash(html)))
        );
        assert!(first.path().with_extension("json").exists());
        assert!(!tmp.path().join("diffs").exists());

        sc.generated_at = "2026-07-28T13:00:00Z".into();
        let second = place(tmp.path(), "repo0000", html, &sc).unwrap();

        assert!(second.is_reused());
        assert_eq!(first.path(), second.path());
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

        assert!(second.is_reused());
        assert_eq!(second.path(), first.path());
        let stored: Sidecar =
            serde_json::from_str(&fs::read_to_string(first.path().with_extension("json")).unwrap())
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

        assert!(second.is_reused());
        assert_eq!(second.path(), first.path());
        let stored: Sidecar =
            serde_json::from_str(&fs::read_to_string(first.path().with_extension("json")).unwrap())
                .unwrap();
        assert_eq!(stored.renderer_version, RENDERER_VERSION);
    }

    #[test]
    fn place_repairs_when_sidecar_deleted_but_html_remains() {
        let tmp = tempfile::tempdir().unwrap();
        let sc = sidecar(DiffKind::TwoDot, "a", "b");
        let first = place(tmp.path(), "repo0000", "<html>repair</html>", &sc).unwrap();
        assert!(!first.is_reused());
        // Delete only the sidecar, leaving the .html behind.
        let json_path = first.path().with_extension("json");
        fs::remove_file(&json_path).unwrap();
        assert!(first.path().exists());
        assert!(!json_path.exists());
        // A subsequent place must rewrite both files, not report reused.
        let repaired = place(tmp.path(), "repo0000", "<html>repair</html>", &sc).unwrap();
        assert!(
            !repaired.is_reused(),
            "should rewrite when sidecar is absent"
        );
        assert!(json_path.exists(), "sidecar must be recreated");
    }

    #[test]
    fn malformed_sidecar_is_not_reused_as_if_it_matched_every_repository() {
        let tmp = tempfile::tempdir().unwrap();
        let sidecar = sidecar(DiffKind::TwoDot, "a", "b");
        let first = place(tmp.path(), "repo0000", "<html>repair</html>", &sidecar).unwrap();
        fs::write(first.path().with_extension("json"), "{not valid json").unwrap();

        let repaired = place(tmp.path(), "repo0000", "<html>repair</html>", &sidecar).unwrap();

        assert!(!repaired.is_reused());
        let stored = fs::read_to_string(repaired.path().with_extension("json")).unwrap();
        assert!(serde_json::from_str::<Sidecar>(&stored).is_ok());
    }

    #[test]
    fn sidecar_with_malformed_timestamp_is_neither_listed_nor_reused() {
        let tmp = tempfile::tempdir().unwrap();
        let sidecar = sidecar(DiffKind::TwoDot, "a", "b");
        let first = place(tmp.path(), "repo0000", "<html>repair</html>", &sidecar).unwrap();
        let mut malformed = sidecar.clone();
        malformed.generated_at = "2026-07-03T00:01:00".into();
        fs::write(
            first.path().with_extension("json"),
            serde_json::to_string(&malformed).unwrap(),
        )
        .unwrap();

        assert!(list_history_with_hash(tmp.path()).is_empty());
        assert!(
            lookup_by_range(
                tmp.path(),
                "repo0000",
                &range_key(DiffKind::TwoDot, "a", "b"),
            )
            .is_none()
        );

        let repaired = place(tmp.path(), "repo0000", "<html>repair</html>", &sidecar).unwrap();
        assert!(!repaired.is_reused());
    }

    #[test]
    fn lookup_by_range_finds_a_commit_range_hit() {
        let tmp = tempfile::tempdir().unwrap();
        let sc = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        place(tmp.path(), "repo0000", "<html>x</html>", &sc).unwrap();
        let hit = lookup_by_range(
            tmp.path(),
            "repo0000",
            &range_key(DiffKind::TwoDot, "aaaa", "bbbb"),
        );
        assert!(hit.is_some());
        let miss = lookup_by_range(
            tmp.path(),
            "repo0000",
            &range_key(DiffKind::TwoDot, "aaaa", "cccc"),
        );
        assert!(miss.is_none());
    }

    #[test]
    fn lookup_by_range_uses_sidecar_repo_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sc = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        sc.repo_id = repository_id("repoBBBB").to_string();
        place(tmp.path(), "repoBBBB", "<html>x</html>", &sc).unwrap();

        assert!(
            lookup_by_range(
                tmp.path(),
                "repoAAAA",
                &range_key(DiffKind::TwoDot, "aaaa", "bbbb"),
            )
            .is_none()
        );
        assert!(
            lookup_by_range(
                tmp.path(),
                "repoBBBB",
                &range_key(DiffKind::TwoDot, "aaaa", "bbbb"),
            )
            .is_some()
        );
    }

    #[test]
    fn lookup_by_range_requires_the_same_exclusion_set() {
        let tmp = tempfile::tempdir().unwrap();
        // An artifact rendered without exclusions cannot satisfy a filtered lookup.
        let sc = sidecar(DiffKind::TwoDot, "aaaa", "bbbb");
        place(tmp.path(), "repo0000", "<html>unfiltered</html>", &sc).unwrap();
        let filtered_key = ArtifactRangeKey {
            excluded_extensions: ExcludedExtensions::new(["md"]),
            ..range_key(DiffKind::TwoDot, "aaaa", "bbbb")
        };
        let filtered = lookup_by_range(tmp.path(), "repo0000", &filtered_key);
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
        let matching_key = ArtifactRangeKey {
            excluded_extensions: ExcludedExtensions::new(["md"]),
            ..range_key(DiffKind::TwoDot, "cccc", "dddd")
        };
        assert!(lookup_by_range(tmp.path(), "repo0000", &matching_key).is_some());
        assert!(
            lookup_by_range(
                tmp.path(),
                "repo0000",
                &range_key(DiffKind::TwoDot, "cccc", "dddd"),
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

        let dark_key = ArtifactRangeKey {
            render_options: RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            theme: Some(Theme::Dark),
            excluded_extensions: ExcludedExtensions::new(["md"]),
            ..range_key(DiffKind::TwoDot, "aaaa", "bbbb")
        };

        assert!(lookup_by_range(tmp.path(), "repo0000", &dark_key).is_some());
        let light_key = ArtifactRangeKey {
            theme: Some(Theme::Light),
            ..dark_key
        };
        assert!(lookup_by_range(tmp.path(), "repo0000", &light_key).is_none());

        let mut legacy = sidecar(DiffKind::TwoDot, "cccc", "dddd");
        legacy.theme_recorded = false;
        place(tmp.path(), "repo0000", "<html>legacy</html>", &legacy).unwrap();
        assert!(
            lookup_by_range(
                tmp.path(),
                "repo0000",
                &range_key(DiffKind::TwoDot, "cccc", "dddd"),
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

        let split_key = ArtifactRangeKey {
            render_options: RenderOptions::new(DiffLayout::Split, DiffDensity::Compact),
            ..range_key(DiffKind::TwoDot, "aaaa", "bbbb")
        };
        let hit = lookup_by_range(tmp.path(), "repo0000", &split_key);
        let miss = lookup_by_range(
            tmp.path(),
            "repo0000",
            &range_key(DiffKind::TwoDot, "aaaa", "bbbb"),
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

        let full_key = ArtifactRangeKey {
            render_options: RenderOptions::new(DiffLayout::Unified, DiffDensity::Full),
            ..range_key(DiffKind::TwoDot, "aaaa", "bbbb")
        };
        let hit = lookup_by_range(tmp.path(), "repo0000", &full_key);
        let miss = lookup_by_range(
            tmp.path(),
            "repo0000",
            &range_key(DiffKind::TwoDot, "aaaa", "bbbb"),
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
            &range_key(DiffKind::TwoDot, "aaaa", "bbbb"),
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
            &range_key(DiffKind::TwoDot, "aaaa", "bbbb"),
        );
        assert!(hit.is_some(), "current renderer artifact must hit");
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
