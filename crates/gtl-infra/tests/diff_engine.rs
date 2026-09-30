#![cfg(test)]

//! Real-git integration tests for application diff slices driven through public
//! interactor boundaries and the `HybridGitClient` adapter. The pure parsing paths
//! are unit-tested in `application`; here we prove the slices against actual Git
//! output on fixture repositories.

use std::sync::Arc;

use gtl_application::{
    diffs::{
        DiffTarget, FileStatus, View,
        compute_commit_patch::{self, ComputeCommitPatch},
        compute_diff::{self, ComputeDiff},
        set_diff_extension_filter::{self, SetDiffExtensionFilter},
    },
    ports::GitClient,
    utils::{
        FixedUserSettingsStore, ProjectComparisons, SavedExtensionFilters, hiding_extensions,
        settings_with_density,
    },
};
use gtl_infra::{git_client::HybridGitClient, testing::TestRepository};
use gtl_models::{
    diffs::{DiffLineCount, ExtensionFilter, ExtensionFilterMode, FileExtensions},
    git::{GitRange, GitRevision},
    paths::RepositoryRelativePath,
    settings::UserSettings,
    viewer::DiffDensity,
};

/// Computes a diff through the production Git adapter with `filter` saved for the repository.
fn compute_view(repository: &TestRepository, target: DiffTarget, filter: ExtensionFilter) -> View {
    compute_view_with_settings(repository, target, filter, UserSettings::default())
}

fn compute_view_with_settings(
    repository: &TestRepository,
    target: DiffTarget,
    filter: ExtensionFilter,
    settings: UserSettings,
) -> View {
    compute_diff::execute(
        ComputeDiff {
            repo_root: repository.root(),
            target,
            changes_since: None,
        },
        &FixedUserSettingsStore::new(settings),
        &HybridGitClient,
        &SavedExtensionFilters::new([(repository.root(), filter)]),
        &ProjectComparisons::default(),
    )
    .unwrap()
    .view
}

/// Computes the patch of the repository's `HEAD` commit.
fn head_commit_patch(repository: &TestRepository) -> View {
    let head = HybridGitClient
        .log_commits(&repository.root(), &GitRange::try_new("HEAD^!").unwrap())
        .unwrap()
        .remove(0);
    compute_commit_patch::execute(
        ComputeCommitPatch {
            repo_root: repository.root(),
            commit: head,
        },
        &FixedUserSettingsStore::default(),
        &HybridGitClient,
        &SavedExtensionFilters::default(),
    )
    .unwrap()
}

fn working_tree() -> DiffTarget {
    DiffTarget::Base(GitRevision::head())
}

fn range(range: &str) -> DiffTarget {
    DiffTarget::Range {
        range: GitRange::try_new(range).unwrap(),
        pinned: None,
    }
}

fn file_paths(view: &View) -> Vec<String> {
    view.files
        .iter()
        .map(|file| file.path.to_string_lossy().into_owned())
        .collect()
}

fn relative_path(path: &str) -> RepositoryRelativePath {
    RepositoryRelativePath::try_new(path.into()).unwrap()
}

fn utf16_le(contents: &str) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend(contents.encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}

fn utf16_be(contents: &str) -> Vec<u8> {
    let mut bytes = vec![0xfe, 0xff];
    bytes.extend(contents.encode_utf16().flat_map(u16::to_be_bytes));
    bytes
}

#[test]
fn utf16_sql_changes_render_as_text_in_commits_and_working_tree() {
    let repository = TestRepository::new();
    let path = "queries/sample.sql";
    let context = "SELECT 10;\nSELECT 20;\nSELECT 30;\nSELECT 40;\nSELECT 50;\nSELECT 60;\n";
    repository.write(path, utf16_le(&format!("SELECT 1;\n{context}")));
    let base = repository.commit_all("add sample query");
    repository.write(path, utf16_le(&format!("SELECT 2;\n{context}")));
    let head = repository.commit_all("change sample query");
    let target = range(&format!("{base}..{head}"));

    let committed = compute_view(&repository, target.clone(), ExtensionFilter::default());
    let file = &committed.files[0];
    let lines = file.lines.iter().collect::<Vec<_>>();
    assert_eq!(file.path, relative_path(path));
    assert_eq!(file.added, DiffLineCount::new(1));
    assert_eq!(file.removed, DiffLineCount::new(1));
    assert!(lines.contains(&"-SELECT 1;"));
    assert!(lines.contains(&"+SELECT 2;"));
    assert!(!lines.contains(&" SELECT 60;"));

    let full = compute_view_with_settings(
        &repository,
        target,
        ExtensionFilter::default(),
        settings_with_density(DiffDensity::Full),
    );
    assert!(
        full.files[0]
            .full_lines
            .as_ref()
            .unwrap()
            .iter()
            .any(|line| line == " SELECT 60;")
    );

    repository.write(path, utf16_le(&format!("SELECT 3;\n{context}")));
    let working = compute_view(&repository, working_tree(), ExtensionFilter::default());
    let lines = working.files[0].lines.iter().collect::<Vec<_>>();
    assert!(lines.contains(&"-SELECT 2;"));
    assert!(lines.contains(&"+SELECT 3;"));
}

#[test]
fn utf16_big_endian_additions_and_deletions_leave_binary_files_binary() {
    let repository = TestRepository::new();
    repository.write("removed.sql", utf16_be("SELECT 1;\n"));
    repository.write("payload.dat", [0, 1, 2]);
    let base = repository.commit_all("add files");
    std::fs::remove_file(repository.path().join("removed.sql")).unwrap();
    repository.write("added.sql", utf16_be("SELECT 2;\n"));
    repository.write("payload.dat", [0, 1, 3]);
    let head = repository.commit_all("change files");

    let view = compute_view(
        &repository,
        range(&format!("{base}..{head}")),
        ExtensionFilter::default(),
    );
    let added = view
        .files
        .iter()
        .find(|file| file.path == relative_path("added.sql"))
        .unwrap();
    let removed = view
        .files
        .iter()
        .find(|file| file.path == relative_path("removed.sql"))
        .unwrap();
    let binary = view
        .files
        .iter()
        .find(|file| file.path == relative_path("payload.dat"))
        .unwrap();
    assert_eq!(added.status(), FileStatus::Added);
    assert!(added.lines.iter().any(|line| line == "+SELECT 2;"));
    assert_eq!(removed.status(), FileStatus::Deleted);
    assert!(removed.lines.iter().any(|line| line == "-SELECT 1;"));
    assert!(
        binary
            .lines
            .iter()
            .any(|line| line.starts_with("Binary files "))
    );
}

#[test]
fn explicit_binary_diff_attribute_keeps_utf16_sql_binary() {
    let repository = TestRepository::new();
    repository.write(".gitattributes", "*.sql -diff\n");
    repository.write("sample.sql", utf16_le("SELECT 1;\n"));
    let base = repository.commit_all("add sample query");
    repository.write("sample.sql", utf16_le("SELECT 2;\n"));
    let head = repository.commit_all("change sample query");

    let view = compute_view(
        &repository,
        range(&format!("{base}..{head}")),
        ExtensionFilter::default(),
    );
    assert!(
        view.files[0]
            .lines
            .iter()
            .any(|line| line.starts_with("Binary files "))
    );
}

#[test]
fn assemble_hides_filtered_extensions_at_the_git_level() {
    let repository = TestRepository::new();
    repository.write("base.txt", "base\n");
    repository.commit_all("base");
    repository.git(&["checkout", "-q", "-b", "feature"]);
    repository.write("code.rs", "fn work() {}\n");
    repository.write("docs plan.MD", "l1\nl2\nl3\n");
    repository.commit_all("feat: work");

    let view = compute_view(
        &repository,
        DiffTarget::Merge {
            base: GitRevision::try_new("main").unwrap(),
            pinned: None,
        },
        hiding_extensions(&["md"]),
    );

    assert_eq!(
        file_paths(&view),
        ["code.rs"],
        "git must not emit the hidden file"
    );
    let total_added = view
        .files
        .iter()
        .fold(DiffLineCount::default(), |total, file| {
            total.saturating_add(file.added)
        });
    assert_eq!(
        total_added.value(),
        1,
        "hidden lines contribute nothing to the totals"
    );
    assert_eq!(
        view.extension_filter.unwrap().hidden_paths,
        [relative_path("docs plan.MD")],
        "hidden paths come from the name-only pass, case-insensitively"
    );
}

#[test]
fn quoted_git_paths_keep_their_names_and_contents() {
    let repository = TestRepository::new();
    repository.commit_all("base");
    for name in ["back\\slash.rs", "café.rs", "notes é.md", "say \"hi\".rs"] {
        repository.write(name, format!("{name}\n"));
    }
    repository.commit_all("quoted names");

    let view = compute_view(
        &repository,
        range("HEAD~1..HEAD"),
        hiding_extensions(&["md"]),
    );

    for file in &view.files {
        let name = file.path.to_str().unwrap();
        assert!(
            file.lines.iter().any(|line| line == format!("+{name}")),
            "{name} lost its contents"
        );
    }
    assert_eq!(
        file_paths(&view),
        ["back\\slash.rs", "café.rs", "say \"hi\".rs"]
    );
    assert_eq!(
        view.extension_filter.unwrap().hidden_paths,
        [relative_path("notes é.md")]
    );
}

#[test]
fn spaced_paths_keep_their_names_through_changes_and_renames() {
    let repository = TestRepository::new();
    repository.git(&["config", "diff.renames", "true"]);
    repository.write("dir b/file.txt", "before\n");
    repository.write("old b/name.txt", "renamed contents\n");
    repository.commit_all("base");
    repository.write("dir b/file.txt", "after\n");
    repository.git(&["mv", "old b/name.txt", "new name.txt"]);
    repository.commit_all("change and rename");

    let view = compute_view(
        &repository,
        range("HEAD~1..HEAD"),
        ExtensionFilter::default(),
    );

    assert_eq!(file_paths(&view), ["dir b/file.txt", "new name.txt"]);
    assert_eq!(view.files[1].status(), FileStatus::Renamed);
}

#[test]
fn user_diff_configuration_does_not_change_the_parsed_diff() {
    let repository = TestRepository::new();
    repository.write("code.rs", "before\n");
    repository.write("notes.md", "before\n");
    repository.commit_all("base");
    repository.write("code.rs", "after\n");
    repository.write("notes.md", "after\n");
    for (key, value) in [
        ("diff.noprefix", "true"),
        ("diff.mnemonicPrefix", "true"),
        ("color.ui", "always"),
        ("diff.external", "false"),
    ] {
        repository.git(&["config", key, value]);
    }

    let view = compute_view(&repository, working_tree(), hiding_extensions(&["md"]));

    assert_eq!(file_paths(&view), ["code.rs"]);
    assert!(view.files[0].lines.iter().collect::<Vec<_>>().ends_with(&[
        "--- a/code.rs",
        "+++ b/code.rs",
        "@@ -1 +1 @@",
        "-before",
        "+after",
        "",
    ]));
    assert_eq!(
        view.extension_filter.unwrap().hidden_paths,
        [relative_path("notes.md")]
    );
}

#[test]
fn commit_patch_matches_root_and_first_parent_git_semantics() {
    let repository = TestRepository::new();
    repository.write("f.txt", "root\n");
    repository.commit_all("root");

    let root_patch = head_commit_patch(&repository);

    assert_eq!(
        (
            root_patch.files[0].added.value(),
            root_patch.files[0].removed.value()
        ),
        (1, 0)
    );

    repository.write("f.txt", "root\nselected\n");
    repository.commit_all("selected");

    let selected_patch = head_commit_patch(&repository);

    assert_eq!(
        (
            selected_patch.files[0].added.value(),
            selected_patch.files[0].removed.value()
        ),
        (1, 0)
    );
    assert!(
        selected_patch.files[0]
            .lines
            .iter()
            .any(|line| line == "+selected")
    );
}

#[test]
fn working_tree_diff_includes_untracked_without_mutating_the_index() {
    let repository = TestRepository::new();
    repository.write("tracked.txt", "original\n");
    repository.write("deleted.txt", "deleted\n");
    repository.write(".gitignore", "ignored.txt\n");
    repository.commit_all("initial");
    repository.write("tracked.txt", "staged\n");
    repository.git(&["add", "tracked.txt"]);
    repository.write("tracked.txt", "working\n");
    repository.git(&["rm", "-q", "deleted.txt"]);
    repository.write("new file.txt", "untracked\n");
    repository.write("binary.dat", [0_u8, 1, 2]);
    repository.write("ignored.txt", "ignored\n");
    let index_before = std::fs::read(repository.path().join(".git/index")).unwrap();

    let view = compute_view(&repository, working_tree(), ExtensionFilter::default());

    assert_eq!(
        file_paths(&view),
        ["binary.dat", "deleted.txt", "new file.txt", "tracked.txt"]
    );
    assert_eq!(
        std::fs::read(repository.path().join(".git/index")).unwrap(),
        index_before
    );
    let committed = compute_view(&repository, range("HEAD..HEAD"), ExtensionFilter::default());
    assert!(committed.files.is_empty());
}

#[test]
fn initial_working_tree_diff_handles_staged_and_untracked_files() {
    let repository = TestRepository::new();
    repository.write("staged.txt", "first\n");
    repository.git(&["add", "staged.txt"]);
    repository.write("staged.txt", "latest\n");
    repository.write("untracked.txt", "new\n");
    let index_before = std::fs::read(repository.path().join(".git/index")).unwrap();

    let view = compute_view(&repository, working_tree(), ExtensionFilter::default());

    assert!(view.commits.is_empty());
    assert_eq!(view.files.len(), 2);
    assert!(
        view.files
            .iter()
            .all(|file| file.status() == FileStatus::Added)
    );
    assert_eq!(
        std::fs::read(repository.path().join(".git/index")).unwrap(),
        index_before
    );
}

/// Commits three files, then changes each so every extension has a working-tree diff.
fn reveal_fixture() -> TestRepository {
    let repository = TestRepository::new();
    for path in ["code.rs", "Cargo.lock", "docs plan.MD"] {
        repository.write(path, "base\n");
    }
    repository.commit_all("base");
    repository.write("code.rs", "reviewed source\n");
    repository.write("Cargo.lock", "initial lock\n");
    repository.write("docs plan.MD", "hidden docs\n");
    repository
}

fn apply_filter(view: View, mode: ExtensionFilterMode, extensions: &[&str]) -> View {
    set_diff_extension_filter::execute(
        SetDiffExtensionFilter {
            view: Arc::new(view),
            filter: ExtensionFilter::new(mode, FileExtensions::new(extensions)),
        },
        &HybridGitClient,
    )
    .unwrap()
}

#[test]
fn revealing_extensions_preserves_loaded_sources_and_reuses_hidden_contents() {
    for density in [DiffDensity::Compact, DiffDensity::Full] {
        let repository = reveal_fixture();
        let original = compute_view_with_settings(
            &repository,
            working_tree(),
            hiding_extensions(&["lock", "md"]),
            settings_with_density(density),
        );
        assert_eq!(original.files.len(), 1);
        let reviewed = original.files[0].clone();
        repository.write("code.rs", "later source\n");
        repository.write("Cargo.lock", "revealed lock\n");

        let revealed = apply_filter(original, ExtensionFilterMode::Hide, &["md"]);

        assert_eq!(file_paths(&revealed), ["Cargo.lock", "code.rs"]);
        assert_eq!(revealed.files[1], reviewed);
        let lock = revealed.files[0].clone();
        assert!(lock.lines.iter().any(|line| line.contains("revealed lock")));

        let hidden = apply_filter(revealed, ExtensionFilterMode::Hide, &["lock", "rs", "md"]);

        assert!(hidden.files.is_empty());
        assert!(
            hidden.has_diff_content(),
            "a fully hidden diff must remain available"
        );

        repository.write("Cargo.lock", "later lock\n");
        let restored = apply_filter(hidden, ExtensionFilterMode::Hide, &["md"]);

        assert_eq!(restored.files, vec![lock, reviewed.clone()]);
        assert_eq!(
            restored.extension_filter.as_ref().unwrap().hidden_paths,
            [relative_path("docs plan.MD")]
        );

        let only_rust = apply_filter(restored, ExtensionFilterMode::Only, &["rs"]);

        assert_eq!(only_rust.files, vec![reviewed]);
        assert_eq!(
            only_rust.extension_filter.unwrap().hidden_paths,
            [relative_path("Cargo.lock"), relative_path("docs plan.MD")]
        );
    }
}
