#![cfg(test)]

use gtl_application::{
    diffs::{Cmd, FileDiff, Foot, View},
    ports::{ArtifactMeta, ArtifactRangeKey, ArtifactStore, HtmlRenderer},
};
use gtl_artifacts::ArtifactRenderer;
use gtl_infra::artifact_store::StoreArtifacts;
use gtl_models::{
    artifacts::{ArtifactCommitRange, ArtifactDiffIdentity, ArtifactRangeKind},
    diffs::{DiffKind, DiffLineCount, ExcludedExtensions, PinnedRange},
    git::{BranchName, GitHead, GitRevision},
    paths::{ProjectName, RepositoryRelativePath, RepositoryRoot},
    viewer::{DiffDensity, DiffLayout, RenderOptions},
};

fn repository_root(path: &std::path::Path) -> RepositoryRoot {
    RepositoryRoot::try_new(path.to_path_buf()).unwrap()
}

fn pinned_range() -> PinnedRange {
    PinnedRange {
        base: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .try_into()
            .unwrap(),
        head: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .try_into()
            .unwrap(),
    }
}

fn view(repo_root: &RepositoryRoot) -> View {
    View {
        file_filter: gtl_application::diffs::file_filter::DiffFileFilter::default(),
        repo_name: ProjectName::try_from("git-tools").unwrap(),
        repo_root: repo_root.clone(),
        branch: GitHead::Branch(BranchName::try_new("feature").unwrap()),
        upstream: GitRevision::try_new("origin/main").unwrap(),
        commits: Vec::new(),
        files: vec![FileDiff {
            path: RepositoryRelativePath::try_new("src/lib.rs".into()).unwrap(),
            added: DiffLineCount::new(1),
            removed: DiffLineCount::new(1),
            lines: vec!["@@ -1 +1 @@".into(), "-old".into(), "+new".into()].into(),
            full_lines: Some(
                vec![
                    "@@ -1,2 +1,2 @@".into(),
                    "-old".into(),
                    "+new".into(),
                    " context".into(),
                ]
                .into(),
            ),
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
        full_context: gtl_application::diffs::FullContextDiffState::Loaded,
        exclusions: None,
    }
}

fn artifact_meta(repo_root: &RepositoryRoot, render_options: RenderOptions) -> ArtifactMeta {
    ArtifactMeta {
        repo_root: repo_root.clone(),
        repo_name: ProjectName::try_from("git-tools").unwrap(),
        identity: ArtifactDiffIdentity::from_parts(DiffKind::TwoDot, Some(pinned_range())).unwrap(),
        range_label: "aaaa..bbbb".into(),
        head_committed_at: Some(
            gtl_models::timestamps::MachineTimestamp::try_from("2026-07-21T00:00:00Z").unwrap(),
        ),
        generated_at: gtl_models::timestamps::MachineTimestamp::try_from("2026-07-21T00:01:00Z")
            .unwrap(),
        title: "diff".into(),
        render_options,
        theme: None,
        excluded_extensions: ExcludedExtensions::default(),
    }
}

fn artifact_range_key(render_options: RenderOptions) -> ArtifactRangeKey {
    ArtifactRangeKey {
        range: ArtifactCommitRange {
            kind: ArtifactRangeKind::TwoDot,
            commits: pinned_range(),
        },
        render_options,
        theme: None,
        excluded_extensions: ExcludedExtensions::default(),
    }
}

#[test]
fn presentation_options_have_distinct_artifact_identities() {
    let repo = tempfile::tempdir().unwrap();
    let repo_root = repository_root(repo.path());
    let store_root = tempfile::tempdir().unwrap();
    let view = view(&repo_root);
    let options_default = RenderOptions::DEFAULT;
    let options_split_full = RenderOptions::new(DiffLayout::Split, DiffDensity::Full);
    let html_default = ArtifactRenderer
        .build_html(&view, options_default, None)
        .unwrap();
    let html_split_full = ArtifactRenderer
        .build_html(&view, options_split_full, None)
        .unwrap();

    assert_ne!(html_default, html_split_full);

    let artifact_default = StoreArtifacts
        .place(
            store_root.path(),
            &artifact_meta(&repo_root, options_default),
            &html_default,
        )
        .unwrap();
    let artifact_split_full = StoreArtifacts
        .place(
            store_root.path(),
            &artifact_meta(&repo_root, options_split_full),
            &html_split_full,
        )
        .unwrap();

    assert_ne!(artifact_default.path(), artifact_split_full.path());
    assert_eq!(
        StoreArtifacts
            .lookup_by_range(
                store_root.path(),
                &repo_root,
                &artifact_range_key(options_default),
            )
            .unwrap(),
        Some(artifact_default.path().clone()),
    );
    assert_eq!(
        StoreArtifacts
            .lookup_by_range(
                store_root.path(),
                &repo_root,
                &artifact_range_key(options_split_full),
            )
            .unwrap(),
        Some(artifact_split_full.path().clone()),
    );
}
