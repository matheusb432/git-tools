#![cfg(test)]

use std::path::Path;

use gtl_application::{
    diffs::{
        DiffTarget,
        compute_diff::{self, ComputeDiff},
    },
    ports::ProjectComparisonReader,
    projects::{
        catalogue::create_project, comparison, select_comparison_repositories,
        update_project_comparison,
    },
    recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget},
    utils::FixedUserSettingsStore,
    viewer::{ViewerState, refresh_live_view, set_viewer_tab_live, work},
};
use gtl_infra::{app_state::SqliteAppState, git_client::HybridGitClient, testing::TestRepository};
use gtl_models::{
    paths::RepositoryRoot,
    projects::{
        catalogue::{ProjectGroups, ProjectMetadata},
        comparison::ComparisonBranch,
    },
    recipes::RecipeBatchId,
    viewer::ViewerTabId,
};
use gtl_wire::viewer::{FieldUpdate, SetViewerTabLive};

/// A repository whose `feature` branch is one commit ahead of `main`, beside an app-state database.
struct Fixture {
    directory: tempfile::TempDir,
    database: SqliteAppState,
    repository: TestRepository,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::Builder::new()
            .prefix(".gtl-comparison-")
            .tempdir()
            .unwrap();
        let repository = TestRepository::init(directory.path().join("repository"));
        repository.write("base.txt", "base\n");
        repository.commit_all("base");
        repository.git(&["checkout", "-qb", "feature"]);
        repository.write("feature.txt", "feature\n");
        repository.commit_all("feature");
        let database = SqliteAppState::open(&directory.path().join("state")).unwrap();
        Self {
            directory,
            database,
            repository,
        }
    }

    fn register(&self, id: &str, title: &str, path: &RepositoryRoot) {
        let source = path.to_string();
        create_project::execute(
            &create_project::CreateProject {
                id: id.to_owned().try_into().unwrap(),
                metadata: ProjectMetadata {
                    title: title.to_owned().try_into().unwrap(),
                    source: source.try_into().unwrap(),
                    git_remote: None,
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
        update_project_comparison::execute(
            update_project_comparison::UpdateProjectComparison {
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
            source: RecipeSource::LocalRepo(self.repository.root()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        }
    }

    fn open(&self, viewer: &ViewerState) -> ViewerTabId {
        let reserved =
            work::reserve_open(viewer, self.recipe(), RecipeBatchId::generate()).unwrap();
        let tab = reserved.ticket().tab_id;
        let computed = work::compute_recipe(
            reserved,
            &FixedUserSettingsStore::default(),
            &HybridGitClient,
            &self.database,
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
    let repository = &fixture.repository;
    repository.git(&["checkout", "-q", "main"]);
    repository.write("main-only.txt", "main\n");
    repository.commit_all("advance main");
    repository.git(&["checkout", "-q", "feature"]);
    repository.write("base.txt", "staged\n");
    repository.git(&["add", "base.txt"]);
    repository.write("feature.txt", "unstaged\n");
    repository.write("untracked.txt", "untracked\n");
    let response = compute_diff::execute(
        ComputeDiff {
            repo_root: repository.root(),
            target: DiffTarget::Unpushed { pinned: None },
            changes_since: None,
        },
        &FixedUserSettingsStore::default(),
        &HybridGitClient,
        &fixture.database,
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
}

#[test]
fn persisted_settings_inherit_across_worktrees_and_explicit_registration_wins() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let root = repository.root();
    fixture.register("PRJ", "project", &root);
    assert_eq!(
        fixture.database.comparison_branch(&root).unwrap(),
        Some(ComparisonBranch::default())
    );
    repository.git(&["branch", "develop", "main"]);
    fixture.set_branch("project", "main", "develop");
    let linked = fixture.directory.path().join("linked");
    repository.git(&["worktree", "add", "-qb", "review", linked.to_str().unwrap()]);
    let linked = RepositoryRoot::try_new(linked).unwrap();
    assert_eq!(
        comparison::configured_comparison(&linked, &HybridGitClient, &fixture.database).unwrap(),
        comparison::ConfiguredComparison {
            branch: ComparisonBranch::try_new("develop").unwrap(),
            project: Some(root.clone()),
        }
    );
    fixture.register("WT", "linked", &linked);
    assert_eq!(
        comparison::configured_comparison(&linked, &HybridGitClient, &fixture.database).unwrap(),
        comparison::ConfiguredComparison {
            branch: ComparisonBranch::default(),
            project: Some(linked.clone()),
        }
    );
    let reopened = SqliteAppState::open(&fixture.directory.path().join("state")).unwrap();
    assert_eq!(
        reopened.comparison_branch(&root).unwrap().unwrap().as_ref(),
        "develop"
    );
}

#[test]
fn upstream_wins_and_tags_cannot_satisfy_a_local_comparison_branch() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let root = repository.root();
    fixture.register("PRJ", "project", &root);
    repository.git(&["tag", "only-tag"]);
    fixture.set_branch("project", "main", "only-tag");
    assert!(matches!(
        comparison::resolve(&root, &HybridGitClient, &fixture.database),
        Err(comparison::ComparisonError::MissingBranch { project: Some(project), .. })
            if project == root
    ));
    repository.git(&["branch", "--set-upstream-to=main"]);
    assert!(matches!(
        comparison::resolve(&root, &HybridGitClient, &fixture.database).unwrap(),
        comparison::ResolvedComparison::Upstream { .. }
    ));
}

#[test]
fn live_tabs_follow_setting_and_base_tip_changes_while_other_tabs_stay_pinned() -> anyhow::Result<()>
{
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let root = repository.root();
    fixture.register("PRJ", "project", &root);
    repository.git(&["branch", "develop", "feature"]);
    let live = ViewerState::new();
    let tab = fixture.open(&live);
    set_viewer_tab_live::execute(
        SetViewerTabLive {
            tab_id: tab,
            live: true,
        },
        &live,
    )?;
    let snapshot = ViewerState::new();
    let snapshot_tab = fixture.open(&snapshot);
    let pinned = snapshot
        .inspect(|session| session.tab(snapshot_tab).unwrap().recipe.clone())
        .unwrap();
    assert!(matches!(
        pinned.op,
        RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: Some(_) }
        }
    ));
    let prepare = || {
        refresh_live_view::prepare(
            tab,
            &live,
            &FixedUserSettingsStore::default(),
            &HybridGitClient,
            &fixture.database,
            &fixture.database,
        )
        .unwrap()
    };
    assert!(matches!(
        prepare(),
        refresh_live_view::LiveViewCheck::Unchanged
    ));
    fixture.set_branch("project", "main", "develop");
    let refresh_live_view::LiveViewCheck::Prepared(publication) = prepare() else {
        anyhow::bail!("setting change must update the live tab")
    };
    refresh_live_view::publish(*publication, &live).unwrap();
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
    repository.git(&["branch", "-f", "develop", "main"]);
    let refresh_live_view::LiveViewCheck::Prepared(publication) = prepare() else {
        anyhow::bail!("base tip change must update the live tab")
    };
    refresh_live_view::publish(*publication, &live).unwrap();
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
    let reserved = work::reserve_open(&snapshot, pinned, RecipeBatchId::generate()).unwrap();
    let computed = work::compute_recipe(
        reserved,
        &FixedUserSettingsStore::default(),
        &HybridGitClient,
        &fixture.database,
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
    let repository = &fixture.repository;
    let root = repository.root();
    fixture.register("PRJ", "project", &root);
    repository.git(&["checkout", "--orphan", "unrelated"]);
    repository.commit_all("unrelated root");
    repository.git(&["checkout", "-q", "feature"]);
    fixture.set_branch("project", "main", "unrelated");
    assert!(matches!(
        comparison::resolve(&root, &HybridGitClient, &fixture.database),
        Err(comparison::ComparisonError::NoCommonAncestor { .. })
    ));
    let result = select_comparison_repositories::execute(
        vec![gtl_models::projects::ProjectRepository {
            name: "project".to_owned().try_into().unwrap(),
            path: root,
            remote: None,
        }],
        &HybridGitClient,
        &fixture.database,
    )
    .unwrap();
    assert_eq!(
        result.repositories,
        Vec::<gtl_models::repository::traversal::RepositoryTarget>::new()
    );
    assert_eq!(result.notes.len(), 1);
    let conflict = update_project_comparison::execute(
        update_project_comparison::UpdateProjectComparison {
            project_name: "project".to_owned().try_into().unwrap(),
            comparison_branch: FieldUpdate::Update(ComparisonBranch::default()),
            expected_comparison_branch: ComparisonBranch::default(),
        },
        &fixture.database.connection_lock().unwrap(),
    );
    assert!(matches!(
        conflict,
        Err(update_project_comparison::UpdateProjectComparisonError::Conflict)
    ));
}
