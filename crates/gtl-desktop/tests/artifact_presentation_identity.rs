#![cfg(test)]

use std::path::Path;

use gtl_application::{
    diffs::{Cmd, FileDiff, Foot, View},
    ports::{ArtifactMeta, ArtifactRangeKey, ArtifactStore, HtmlRenderer},
};
use gtl_artifacts::ArtifactRenderer;
use gtl_infra::artifact_store::StoreArtifacts;
use gtl_models::{
    diffs::{DiffKind, PinnedRange},
    viewer::{DiffDensity, DiffLayout, RenderOptions},
};

fn pinned_range() -> PinnedRange {
    PinnedRange {
        base: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .try_into()
            .expect("fixture base commit ID is valid"),
        head: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .try_into()
            .expect("fixture head commit ID is valid"),
    }
}

fn view(repo_root: &Path) -> View {
    View {
        repo_name: "git-tools".into(),
        repo_root: repo_root.to_string_lossy().into_owned(),
        branch: "feature".into(),
        upstream: "origin/main".into(),
        commits: Vec::new(),
        files: vec![FileDiff {
            path: "src/lib.rs".into(),
            added: 1,
            removed: 1,
            lines: vec!["@@ -1 +1 @@".into(), "-old".into(), "+new".into()],
            full_lines: Some(vec![
                "@@ -1,2 +1,2 @@".into(),
                "-old".into(),
                "+new".into(),
                " context".into(),
            ]),
        }],
        title: "diff".into(),
        cmd: Cmd {
            lead: "git diff ".into(),
            range: "aaaa..bbbb".into(),
            trail: String::new(),
        },
        commits_label: "Commits".into(),
        foot: Foot {
            cmd: "git diff aaaa..bbbb".into(),
        },
        exclusions: None,
    }
}

fn artifact_meta(repo_root: &Path, render_options: RenderOptions) -> ArtifactMeta {
    ArtifactMeta {
        repo_root: repo_root.to_path_buf(),
        repo_name: "git-tools".into(),
        kind: DiffKind::TwoDot,
        commit_range: Some(pinned_range()),
        range_label: "aaaa..bbbb".into(),
        head_committed_at: "2026-07-21T00:00:00Z".into(),
        generated_at: "2026-07-21T00:01:00Z".into(),
        title: "diff".into(),
        render_options,
        theme: None,
        excluded_extensions: Vec::new(),
    }
}

fn artifact_range_key(render_options: RenderOptions) -> ArtifactRangeKey {
    ArtifactRangeKey {
        kind: DiffKind::TwoDot,
        commit_range: pinned_range(),
        render_options,
        theme: None,
        excluded_extensions: Vec::new(),
    }
}

#[test]
fn presentation_options_have_distinct_artifact_identities() {
    let repo = tempfile::tempdir().unwrap();
    let store_root = tempfile::tempdir().unwrap();
    let view = view(repo.path());
    let options_default = RenderOptions::DEFAULT;
    let options_split_full = RenderOptions::new(DiffLayout::Split, DiffDensity::Full);
    let html_default = ArtifactRenderer
        .build_html(&view, options_default, None)
        .expect("embedded syntax assets should load");
    let html_split_full = ArtifactRenderer
        .build_html(&view, options_split_full, None)
        .expect("embedded syntax assets should load");

    assert_ne!(html_default, html_split_full);

    let artifact_default = StoreArtifacts
        .place(
            store_root.path(),
            &artifact_meta(repo.path(), options_default),
            &html_default,
        )
        .unwrap();
    let artifact_split_full = StoreArtifacts
        .place(
            store_root.path(),
            &artifact_meta(repo.path(), options_split_full),
            &html_split_full,
        )
        .unwrap();

    assert_ne!(artifact_default.path, artifact_split_full.path);
    assert_eq!(
        StoreArtifacts
            .lookup_by_range(
                store_root.path(),
                repo.path(),
                &artifact_range_key(options_default),
            )
            .unwrap(),
        Some(artifact_default.path),
    );
    assert_eq!(
        StoreArtifacts
            .lookup_by_range(
                store_root.path(),
                repo.path(),
                &artifact_range_key(options_split_full),
            )
            .unwrap(),
        Some(artifact_split_full.path),
    );
}
