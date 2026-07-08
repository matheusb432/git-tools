//! Native-tab commands: open/refresh/close a recipe tab, read its meta, page
//! its rows. Inner functions are plain and testable; the `#[tauri::command]`
//! wrappers only extract state, clone it, and hop to a blocking worker.

use std::path::Path;

use application::{
    diffs::{
        compute_diff::ComputeDiff, compute_merge_diff::ComputeMergeDiff,
        compute_squash_preview::ComputeSquashPreview,
    },
    history::record_render::RecordRender,
};
use cqrsy::{Handle, Sender};
use domain::diffs::View;
use serde::Serialize;

use crate::{
    recipe::{Recipe, RecipeOp},
    tabs::{Layout, RenderedTabs, RowsPage},
    view_dto::TabMeta,
};

/// What `open_recipe` answers: the (possibly reused) tab and its fresh meta.
#[derive(Debug, Serialize)]
pub(crate) struct OpenedTab {
    pub tab_id: u64,
    pub meta: TabMeta,
}

/// Dispatch the recipe's compute slice through the mediator (sync seam).
fn compute_view<M>(mediator: &M, recipe: &Recipe) -> Result<View, String>
where
    M: Sender<ComputeDiff> + Sender<ComputeMergeDiff> + Sender<ComputeSquashPreview> + Handle,
{
    let cwd = recipe.cwd();
    match &recipe.op {
        RecipeOp::Diff { target } => mediator
            .send_now(ComputeDiff {
                cwd,
                target: target.clone().into(),
            })
            .map(|response| response.view)
            .map_err(|err| format!("{err:#}")),
        RecipeOp::MergeDiff { base } => mediator
            .send_now(ComputeMergeDiff {
                cwd,
                base: base.clone(),
            })
            .map(|response| response.view)
            .map_err(|err| format!("{err:#}")),
        RecipeOp::SquashPreview => mediator
            .send_now(ComputeSquashPreview { cwd })
            .map(|response| response.view)
            .map_err(|err| format!("{err:#}")),
    }
}

pub(crate) fn open_recipe_inner<M>(
    mediator: &M,
    tabs: &RenderedTabs,
    data_root: &Path,
    recipe: Recipe,
    batch_id: String,
) -> Result<OpenedTab, String>
where
    M: Sender<ComputeDiff>
        + Sender<ComputeMergeDiff>
        + Sender<ComputeSquashPreview>
        + Sender<RecordRender>
        + Handle,
{
    let view = compute_view(mediator, &recipe)?;
    let record = RecordRender {
        data_root: data_root.to_path_buf(),
        recipe_json: serde_json::to_string(&recipe).map_err(|err| err.to_string())?,
        title: view.title.clone(),
        repo_name: view.repo_name.clone(),
        kind: recipe.kind_tag().to_string(),
        range_label: view.cmd.range.clone(),
    };
    let tab_id = tabs.upsert(recipe, batch_id, view);
    // History is auxiliary: a failed write must not kill the tab.
    if let Err(err) = mediator.send_now(record) {
        eprintln!("gtl-viewer: failed to record render history: {err:#}");
    }
    let meta = tabs
        .meta(tab_id)
        .ok_or_else(|| "tab closed during open".to_string())?;
    Ok(OpenedTab { tab_id, meta })
}

pub(crate) fn refresh_tab_inner<M>(
    mediator: &M,
    tabs: &RenderedTabs,
    tab_id: u64,
) -> Result<TabMeta, String>
where
    M: Sender<ComputeDiff> + Sender<ComputeMergeDiff> + Sender<ComputeSquashPreview> + Handle,
{
    let recipe = tabs
        .recipe(tab_id)
        .ok_or_else(|| format!("unknown tab {tab_id}"))?;
    let view = compute_view(mediator, &recipe)?;
    if !tabs.replace_view(tab_id, view) {
        return Err(format!("unknown tab {tab_id}"));
    }
    tabs.meta(tab_id)
        .ok_or_else(|| "tab closed during refresh".to_string())
}

#[tauri::command]
pub(crate) async fn open_recipe(
    mediator: tauri::State<'_, crate::WiredMediator>,
    tabs: tauri::State<'_, RenderedTabs>,
    recipe: Recipe,
    batch_id: String,
) -> Result<OpenedTab, String> {
    let mediator = mediator.inner().clone();
    let tabs = tabs.inner().clone();
    let data_root = super::data_root()?;
    tauri::async_runtime::spawn_blocking(move || {
        open_recipe_inner(&mediator, &tabs, &data_root, recipe, batch_id)
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub(crate) async fn refresh_tab(
    mediator: tauri::State<'_, crate::WiredMediator>,
    tabs: tauri::State<'_, RenderedTabs>,
    tab_id: u64,
) -> Result<TabMeta, String> {
    let mediator = mediator.inner().clone();
    let tabs = tabs.inner().clone();
    tauri::async_runtime::spawn_blocking(move || refresh_tab_inner(&mediator, &tabs, tab_id))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub(crate) async fn tab_meta(
    tabs: tauri::State<'_, RenderedTabs>,
    tab_id: u64,
) -> Result<TabMeta, String> {
    tabs.meta(tab_id)
        .ok_or_else(|| format!("unknown tab {tab_id}"))
}

#[tauri::command]
pub(crate) async fn file_rows(
    tabs: tauri::State<'_, RenderedTabs>,
    tab_id: u64,
    file_idx: usize,
    layout: Layout,
    full: bool,
    start: usize,
    count: usize,
) -> Result<RowsPage, String> {
    let tabs = tabs.inner().clone();
    // Row derivation for a huge file is the one meta-path cost worth hopping
    // off the IPC task for.
    tauri::async_runtime::spawn_blocking(move || {
        tabs.rows_page(tab_id, file_idx, layout, full, start, count)
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub(crate) async fn close_tab(
    tabs: tauri::State<'_, RenderedTabs>,
    tab_id: u64,
) -> Result<bool, String> {
    Ok(tabs.close(tab_id))
}

#[cfg(test)]
mod tests {
    use application::testing::{FakeDiffSource, InMemoryAppStateStore, InMemoryArtifactStore};

    use super::*;
    use crate::recipe::{RecipeSource, RecipeTarget};

    const SINGLE_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,2 +1,3 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n";

    fn diff_source() -> FakeDiffSource {
        FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![domain::diffs::Commit {
                sha: "abc1234".into(),
                subject: "feat: work".into(),
                ..Default::default()
            }],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        }
    }

    fn unpushed_recipe() -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo("/repo".into()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed,
            },
        }
    }

    #[test]
    fn open_records_history_and_returns_the_tab_meta() {
        let app_state = InMemoryAppStateStore::default();
        let mediator = crate::test_support::fake_mediator_with(
            diff_source(),
            InMemoryArtifactStore::default(),
            app_state.clone(),
        );
        let tabs = RenderedTabs::default();

        let opened = open_recipe_inner(
            &mediator,
            &tabs,
            Path::new("/data"),
            unpushed_recipe(),
            "batch-1".into(),
        )
        .expect("open succeeds");

        assert_eq!(opened.meta.repo_name, "repo");
        assert_eq!(opened.meta.files.len(), 1);
        assert_eq!(opened.meta.batch_id, "batch-1");

        let renders = app_state.renders.lock().unwrap();
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].kind, "diff");
        let recorded: Recipe = serde_json::from_str(&renders[0].recipe_json).unwrap();
        assert_eq!(recorded, unpushed_recipe());
    }

    #[test]
    fn reopening_the_same_recipe_reuses_the_tab_id() {
        let mediator = crate::test_support::fake_mediator(diff_source());
        let tabs = RenderedTabs::default();
        let data = Path::new("/data");

        let first =
            open_recipe_inner(&mediator, &tabs, data, unpushed_recipe(), "b1".into()).unwrap();
        let second =
            open_recipe_inner(&mediator, &tabs, data, unpushed_recipe(), "b2".into()).unwrap();

        assert_eq!(first.tab_id, second.tab_id);
        assert_eq!(second.meta.batch_id, "b2");
    }

    #[test]
    fn refresh_recomputes_and_keeps_the_tab() {
        let mediator = crate::test_support::fake_mediator(diff_source());
        let tabs = RenderedTabs::default();
        let opened = open_recipe_inner(
            &mediator,
            &tabs,
            Path::new("/data"),
            unpushed_recipe(),
            "b1".into(),
        )
        .unwrap();

        let meta = refresh_tab_inner(&mediator, &tabs, opened.tab_id).expect("refresh succeeds");

        assert_eq!(meta.tab_id, opened.tab_id);
        assert_eq!(meta.files.len(), 1);
    }

    #[test]
    fn open_pages_marker_free_rows_through_the_cache() {
        let mediator = crate::test_support::fake_mediator(diff_source());
        let tabs = RenderedTabs::default();
        let opened = open_recipe_inner(
            &mediator,
            &tabs,
            Path::new("/data"),
            unpushed_recipe(),
            "b1".into(),
        )
        .unwrap();

        let page = tabs
            .rows_page(opened.tab_id, 0, Layout::Unified, false, 0, 100)
            .expect("page succeeds");

        let crate::tabs::RowsSlice::Unified(rows) = page.slice else {
            panic!("unified expected");
        };
        let add = rows
            .iter()
            .find(|row| row.kind == "add")
            .expect("an add row exists");
        assert_eq!(add.text, "new line", "marker must be stripped");
    }

    #[test]
    fn compute_failure_surfaces_and_records_nothing() {
        let app_state = InMemoryAppStateStore::default();
        let mediator = crate::test_support::fake_mediator_with(
            FakeDiffSource::default(), // top_level: None → "not a git repository"
            InMemoryArtifactStore::default(),
            app_state.clone(),
        );
        let tabs = RenderedTabs::default();

        let error = open_recipe_inner(
            &mediator,
            &tabs,
            Path::new("/data"),
            unpushed_recipe(),
            "b1".into(),
        )
        .expect_err("compute fails");

        assert!(error.contains("not a git repository"));
        assert!(app_state.renders.lock().unwrap().is_empty());
    }

    #[test]
    fn refresh_of_an_unknown_tab_errors() {
        let mediator = crate::test_support::fake_mediator(diff_source());
        let tabs = RenderedTabs::default();
        assert!(refresh_tab_inner(&mediator, &tabs, 99).is_err());
    }
}
