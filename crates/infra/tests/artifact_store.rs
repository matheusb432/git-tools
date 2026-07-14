//! Integration tests for the `StoreArtifacts` adapter against a real git fixture
//! repo + a tempdir store: a placed commit-range artifact is found by
//! `lookup_by_range`, and a `WorkTree` artifact is never range-addressable.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use application::ports::{ArtifactMeta, ArtifactStore};
use domain::diffs::DiffKind;
use infra::artifact_store::StoreArtifacts;

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

/// A one-commit git repo under a fresh tempdir; returns its path.
fn fixture_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().to_path_buf();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "first"]);
    (tmp, repo)
}

fn meta(repo: &Path, kind: DiffKind, base: &str, head: &str) -> ArtifactMeta {
    ArtifactMeta {
        repo_root: repo.to_path_buf(),
        repo_name: "fixture".to_string(),
        kind,
        base_sha: base.to_string(),
        head_sha: head.to_string(),
        range_label: "base..head".to_string(),
        excluded_extensions: Vec::new(),
        head_committed_at: "2026-07-02T00:00:00Z".to_string(),
        generated_at: "2026-07-02T00:00:00Z".to_string(),
        title: "diff".to_string(),
    }
}

#[test]
fn place_then_lookup_by_range_returns_the_placed_artifact() {
    let store = tempfile::tempdir().unwrap();
    let (_repo_tmp, repo) = fixture_repo();

    let placed = StoreArtifacts
        .place(
            store.path(),
            &meta(&repo, DiffKind::TwoDot, "aaaa", "bbbb"),
            "<html>x</html>",
        )
        .unwrap();
    assert!(placed.path.exists());
    assert!(!placed.reused);

    let hit = StoreArtifacts
        .lookup_by_range(store.path(), &repo, DiffKind::TwoDot, "aaaa", "bbbb", &[])
        .unwrap();
    assert_eq!(hit.as_deref(), Some(placed.path.as_path()));

    let miss = StoreArtifacts
        .lookup_by_range(store.path(), &repo, DiffKind::TwoDot, "aaaa", "cccc", &[])
        .unwrap();
    assert!(miss.is_none());
}

#[test]
fn worktree_artifacts_are_never_range_addressable() {
    let store = tempfile::tempdir().unwrap();
    let (_repo_tmp, repo) = fixture_repo();

    StoreArtifacts
        .place(
            store.path(),
            &meta(&repo, DiffKind::WorkTree, "", ""),
            "<html>wt</html>",
        )
        .unwrap();

    let hit = StoreArtifacts
        .lookup_by_range(store.path(), &repo, DiffKind::WorkTree, "", "", &[])
        .unwrap();
    assert!(hit.is_none());
}
