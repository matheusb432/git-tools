#![cfg(test)]

use std::{path::Path, process::Command};

use gtl_application::{
    diffs::{
        DiffTarget,
        compute_diff::{self, ComputeDiff},
    },
    ports::ProjectComparisonReader,
    projects::{
        catalogue::create_project, comparison, select_comparison_repositories,
        update_viewer_project,
    },
    recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget},
    utils::FixedUserSettingsStore,
    viewer::{ViewerState, refresh_live_view, work},
};
use gtl_infra::{app_state::SqliteAppState, git_client::HybridGitClient};
use gtl_models::{
    paths::RepositoryRoot,
    projects::{
        catalogue::{ProjectAffiliation, ProjectGroups, ProjectMetadata},
        comparison::ComparisonBranch,
    },
    recipes::RecipeBatchId,
    viewer::{ViewerTabId, ViewerTabKind},
};
use gtl_wire::viewer::FieldUpdate;

struct Fixture {
    directory: tempfile::TempDir,
    database: SqliteAppState,
    repository: RepositoryRoot,
}

fn git(path: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

impl Fixture {
    fn new() -> Self {
        let home = directories::BaseDirs::new().unwrap();
        let directory = tempfile::Builder::new()
            .prefix(".gtl-comparison-")
            .tempdir_in(home.home_dir())
            .unwrap();
        let path = directory.path().join("repository");
        std::fs::create_dir(&path).unwrap();
        git(&path, &["init", "-q", "-b", "main"]);
        git(&path, &["config", "user.email", "test@example.test"]);
        git(&path, &["config", "user.name", "Test"]);
        std::fs::write(path.join("base.txt"), "base\n").unwrap();
        git(&path, &["add", "."]);
        git(&path, &["commit", "-qm", "base"]);
        git(&path, &["checkout", "-qb", "feature"]);
        std::fs::write(path.join("feature.txt"), "feature\n").unwrap();
        git(&path, &["add", "."]);
        git(&path, &["commit", "-qm", "feature"]);
        let database = SqliteAppState::open(&directory.path().join("state")).unwrap();
        Self {
            directory,
            database,
            repository: RepositoryRoot::try_new(path).unwrap(),
        }
    }

    fn register(&self, id: &str, title: &str, path: &RepositoryRoot) {
        let home = directories::BaseDirs::new().unwrap();
        let source = format!(
            "~/{}",
            path.as_ref()
                .strip_prefix(home.home_dir())
                .unwrap()
                .display()
        );
        create_project::execute(
            &create_project::CreateProject {
                id: id.to_owned().try_into().unwrap(),
                metadata: ProjectMetadata {
                    title: title.to_owned().try_into().unwrap(),
                    source: source.try_into().unwrap(),
                    git_remote: None,
                    mux_session_name: title.to_owned().try_into().unwrap(),
                    affiliation: ProjectAffiliation::Personal,
                    color: None,
                    groups: ProjectGroups::default(),
                },
                include_in_full_export: true,
            },
            &mut self.database.connection_lock().unwrap(),
        )
        .unwrap();
    }

    fn set_branch(&self, title: &str, previous: &str, branch: &str) {
        update_viewer_project::execute(
            update_viewer_project::UpdateProjectComparison {
                project_name: title.to_owned().try_into().unwrap(),
                comparison_branch: FieldUpdate::Update(ComparisonBranch::try_new(branch).unwrap()),
                expected_comparison_branch: ComparisonBranch::try_new(previous).unwrap(),
            },
            &self.database.connection_lock().unwrap(),
        )
        .unwrap();
    }

    fn recipe(&self) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(self.repository.clone()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        }
    }

    fn open(&self, viewer: &ViewerState, kind: ViewerTabKind) -> ViewerTabId {
        let reserved =
            work::reserve_open(viewer, self.recipe(), RecipeBatchId::generate(), kind).unwrap();
        let tab = reserved.ticket().tab_id;
        let computed = work::compute_recipe(
            reserved,
            &FixedUserSettingsStore::default(),
            &HybridGitClient,
            &self.database,
        );
        assert!(matches!(
            work::publish_recipe(viewer, computed).unwrap(),
            work::RecipePublication::Published { .. }
        ));
        tab
    }
}

#[test]
fn fallback_uses_the_common_ancestor_and_excludes_all_uncommitted_changes() {
    let fixture = Fixture::new();
    let path = fixture.repository.as_ref();
    git(path, &["checkout", "-q", "main"]);
    std::fs::write(path.join("main-only.txt"), "main\n").unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "-qm", "advance main"]);
    git(path, &["checkout", "-q", "feature"]);
    std::fs::write(path.join("base.txt"), "staged\n").unwrap();
    git(path, &["add", "base.txt"]);
    std::fs::write(path.join("feature.txt"), "unstaged\n").unwrap();
    std::fs::write(path.join("untracked.txt"), "untracked\n").unwrap();
    let response = compute_diff::execute(
        ComputeDiff {
            repo_root: fixture.repository.clone(),
            target: DiffTarget::Unpushed { pinned: None },
        },
        &FixedUserSettingsStore::default(),
        &HybridGitClient,
        &fixture.database,
    )
    .unwrap();
    assert_eq!(response.view.commits.len(), 1);
    assert_eq!(response.view.files.len(), 1);
    assert_eq!(
        response.view.files[0].path.as_path(),
        Path::new("feature.txt")
    );
    assert_eq!(response.view.cmd.range, "refs/heads/main...HEAD");
    assert_eq!(response.view.commits_label, "# branch changes");
}

#[test]
fn persisted_settings_inherit_across_worktrees_and_explicit_registration_wins() {
    let fixture = Fixture::new();
    fixture.register("PRJ", "project", &fixture.repository);
    assert_eq!(
        fixture
            .database
            .comparison_branch(&fixture.repository)
            .unwrap(),
        Some(ComparisonBranch::default())
    );
    git(fixture.repository.as_ref(), &["branch", "develop", "main"]);
    fixture.set_branch("project", "main", "develop");
    let linked = fixture.directory.path().join("linked");
    git(
        fixture.repository.as_ref(),
        &["worktree", "add", "-qb", "review", linked.to_str().unwrap()],
    );
    let linked = RepositoryRoot::try_new(linked).unwrap();
    assert_eq!(
        comparison::configured_branch(&linked, &HybridGitClient, &fixture.database)
            .unwrap()
            .as_ref(),
        "develop"
    );
    fixture.register("WT", "linked", &linked);
    assert_eq!(
        comparison::configured_branch(&linked, &HybridGitClient, &fixture.database).unwrap(),
        ComparisonBranch::default()
    );
    let reopened = SqliteAppState::open(&fixture.directory.path().join("state")).unwrap();
    assert_eq!(
        reopened
            .comparison_branch(&fixture.repository)
            .unwrap()
            .unwrap()
            .as_ref(),
        "develop"
    );
}

#[test]
fn upstream_wins_and_tags_cannot_satisfy_a_local_comparison_branch() {
    let fixture = Fixture::new();
    fixture.register("PRJ", "project", &fixture.repository);
    git(fixture.repository.as_ref(), &["tag", "only-tag"]);
    fixture.set_branch("project", "main", "only-tag");
    assert!(matches!(
        comparison::resolve(&fixture.repository, &HybridGitClient, &fixture.database),
        Err(comparison::ComparisonError::MissingBranch { .. })
    ));
    git(
        fixture.repository.as_ref(),
        &["branch", "--set-upstream-to=main"],
    );
    assert!(matches!(
        comparison::resolve(&fixture.repository, &HybridGitClient, &fixture.database).unwrap(),
        comparison::ResolvedComparison::Upstream { .. }
    ));
}

#[test]
fn live_diffs_follow_setting_and_base_tip_changes_while_snapshots_stay_pinned() -> anyhow::Result<()>
{
    let fixture = Fixture::new();
    fixture.register("PRJ", "project", &fixture.repository);
    git(
        fixture.repository.as_ref(),
        &["branch", "develop", "feature"],
    );
    let live = ViewerState::new();
    let tab = fixture.open(&live, ViewerTabKind::Live);
    let snapshot = ViewerState::new();
    let snapshot_tab = fixture.open(&snapshot, ViewerTabKind::Snapshot);
    let pinned = snapshot
        .inspect(|session| session.tab(snapshot_tab).unwrap().recipe.clone())
        .unwrap();
    assert!(matches!(
        pinned.op,
        RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: Some(_) }
        }
    ));
    assert!(matches!(
        refresh_live_view::prepare(
            tab,
            &live,
            &FixedUserSettingsStore::default(),
            &HybridGitClient,
            &fixture.database
        )
        .unwrap(),
        refresh_live_view::LiveViewCheck::Unchanged
    ));
    fixture.set_branch("project", "main", "develop");
    let refresh_live_view::LiveViewCheck::Prepared(publication) = refresh_live_view::prepare(
        tab,
        &live,
        &FixedUserSettingsStore::default(),
        &HybridGitClient,
        &fixture.database,
    )
    .unwrap() else {
        anyhow::bail!("setting change must refresh the live comparison")
    };
    refresh_live_view::publish(publication, &live).unwrap();
    assert_eq!(
        live.inspect(|session| session
            .cached_view_snapshot(tab)
            .unwrap()
            .view
            .commits
            .len())
            .unwrap(),
        0
    );
    git(
        fixture.repository.as_ref(),
        &["branch", "-f", "develop", "main"],
    );
    let refresh_live_view::LiveViewCheck::Prepared(publication) = refresh_live_view::prepare(
        tab,
        &live,
        &FixedUserSettingsStore::default(),
        &HybridGitClient,
        &fixture.database,
    )
    .unwrap() else {
        anyhow::bail!("base tip change must refresh the live comparison")
    };
    refresh_live_view::publish(publication, &live).unwrap();
    assert_eq!(
        live.inspect(|session| session
            .cached_view_snapshot(tab)
            .unwrap()
            .view
            .commits
            .len())
            .unwrap(),
        1
    );
    let reserved = work::reserve_open(
        &snapshot,
        pinned,
        RecipeBatchId::generate(),
        ViewerTabKind::Snapshot,
    )
    .unwrap();
    let computed = work::compute_recipe(
        reserved,
        &FixedUserSettingsStore::default(),
        &HybridGitClient,
        &fixture.database,
    );
    work::publish_recipe(&snapshot, computed).unwrap();
    let rebuilt = snapshot
        .inspect(|session| session.cached_view_snapshot(snapshot_tab).unwrap())
        .unwrap();
    assert_eq!(rebuilt.view.commits.len(), 1);
    assert_eq!(rebuilt.view.files.len(), 1);
    Ok(())
}

#[test]
fn invalid_comparisons_are_reported_and_skipped_in_batches() {
    let fixture = Fixture::new();
    fixture.register("PRJ", "project", &fixture.repository);
    git(
        fixture.repository.as_ref(),
        &["checkout", "--orphan", "unrelated"],
    );
    git(
        fixture.repository.as_ref(),
        &["commit", "-qm", "unrelated root"],
    );
    git(fixture.repository.as_ref(), &["checkout", "-q", "feature"]);
    fixture.set_branch("project", "main", "unrelated");
    assert!(matches!(
        comparison::resolve(&fixture.repository, &HybridGitClient, &fixture.database),
        Err(comparison::ComparisonError::NoCommonAncestor { .. })
    ));
    let result = select_comparison_repositories::execute(
        vec![gtl_models::projects::ProjectRepository {
            name: "project".to_owned().try_into().unwrap(),
            path: fixture.repository.clone(),
            remote: None,
        }],
        &HybridGitClient,
        &fixture.database,
    )
    .unwrap();
    assert!(result.repositories.is_empty());
    assert_eq!(result.notes.len(), 1);
    let conflict = update_viewer_project::execute(
        update_viewer_project::UpdateProjectComparison {
            project_name: "project".to_owned().try_into().unwrap(),
            comparison_branch: FieldUpdate::Update(ComparisonBranch::default()),
            expected_comparison_branch: ComparisonBranch::default(),
        },
        &fixture.database.connection_lock().unwrap(),
    );
    assert!(matches!(
        conflict,
        Err(update_viewer_project::UpdateProjectComparisonError::Conflict)
    ));
}
