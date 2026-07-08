//! The in-memory render cache behind native tabs: `tab_id → RenderedTab`
//! (recipe + computed [`View`] + lazily-derived row panes). The frontend never
//! holds a whole diff; commands page bounded slices out of this state. Rows
//! for a (file, layout, full) pane derive on first request and are cached; a
//! refresh/replace drops every cached pane. Derivation happens under the map
//! lock — per-file, millisecond-scale; revisit only if the perf harness says
//! otherwise.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use domain::diffs::{Row, SplitRow, View, split_rows};
use serde::{Deserialize, Serialize};

use crate::{
    recipe::Recipe,
    view_dto::{self, RowDto, SplitRowDto, TabMeta},
};

/// Which pane layout a rows request targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    Unified,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PaneKey {
    file_idx: usize,
    layout: Layout,
    full: bool,
}

enum PaneRows {
    Unified(Vec<Row>),
    Split(Vec<SplitRow>),
}

struct RenderedTab {
    recipe: Recipe,
    batch_id: String,
    view: View,
    panes: HashMap<PaneKey, PaneRows>,
}

#[derive(Default)]
struct TabsInner {
    next_id: u64,
    tabs: HashMap<u64, RenderedTab>,
}

/// Managed Tauri state: every open native tab's computed content.
#[derive(Clone, Default)]
pub struct RenderedTabs(Arc<Mutex<TabsInner>>);

/// One page of rows plus the pane's total row count (the virtualizer's scroll
/// size arrives with the first page — no separate count round-trip).
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct RowsPage {
    pub total: usize,
    #[serde(flatten)]
    pub slice: RowsSlice,
}

/// The paged rows in the shape the requested layout renders.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "layout", content = "rows", rename_all = "kebab-case")]
pub enum RowsSlice {
    Unified(Vec<RowDto>),
    Split(Vec<SplitRowDto>),
}

impl RenderedTabs {
    /// Insert a computed view, or — same recipe identity — replace the existing
    /// tab's content in place (same id, fresh batch, caches dropped).
    pub fn upsert(&self, recipe: Recipe, batch_id: String, view: View) -> u64 {
        let mut inner = self.0.lock().expect("tabs lock");
        if let Some((&id, _)) = inner.tabs.iter().find(|(_, tab)| tab.recipe == recipe) {
            let tab = inner.tabs.get_mut(&id).expect("just found");
            tab.batch_id = batch_id;
            tab.view = view;
            tab.panes.clear();
            return id;
        }
        inner.next_id += 1;
        let id = inner.next_id;
        inner.tabs.insert(
            id,
            RenderedTab {
                recipe,
                batch_id,
                view,
                panes: HashMap::new(),
            },
        );
        id
    }

    /// The tab-shell metadata (summaries, never lines).
    pub fn meta(&self, tab_id: u64) -> Option<TabMeta> {
        let inner = self.0.lock().expect("tabs lock");
        let tab = inner.tabs.get(&tab_id)?;
        Some(view_dto::tab_meta(tab_id, &tab.batch_id, &tab.view))
    }

    /// The recipe a tab renders (refresh recomputes from it).
    pub fn recipe(&self, tab_id: u64) -> Option<Recipe> {
        let inner = self.0.lock().expect("tabs lock");
        inner.tabs.get(&tab_id).map(|tab| tab.recipe.clone())
    }

    /// Swap in a recomputed view (refresh), dropping cached panes. The old
    /// content stays served until this call — the non-blocking refresh contract.
    pub fn replace_view(&self, tab_id: u64, view: View) -> bool {
        let mut inner = self.0.lock().expect("tabs lock");
        let Some(tab) = inner.tabs.get_mut(&tab_id) else {
            return false;
        };
        tab.view = view;
        tab.panes.clear();
        true
    }

    /// Drop a tab. Idempotent: closing an unknown id is `false`, not an error.
    pub fn close(&self, tab_id: u64) -> bool {
        self.0
            .lock()
            .expect("tabs lock")
            .tabs
            .remove(&tab_id)
            .is_some()
    }

    /// A bounded page of one file's rows in one pane variant, deriving and
    /// caching the pane on first request.
    pub fn rows_page(
        &self,
        tab_id: u64,
        file_idx: usize,
        layout: Layout,
        full: bool,
        start: usize,
        count: usize,
    ) -> Result<RowsPage, String> {
        let mut inner = self.0.lock().expect("tabs lock");
        let tab = inner
            .tabs
            .get_mut(&tab_id)
            .ok_or_else(|| format!("unknown tab {tab_id}"))?;
        let file = tab
            .view
            .files
            .get(file_idx)
            .ok_or_else(|| format!("file index {file_idx} out of range"))?;

        let key = PaneKey {
            file_idx,
            layout,
            full,
        };
        let pane = tab.panes.entry(key).or_insert_with(|| {
            // ! full falls back to compact when no full-context lines were
            // ! computed for this file — same policy as the Maud renderer.
            let base = if full {
                file.full_rows().unwrap_or_else(|| file.rows())
            } else {
                file.rows()
            };
            match layout {
                Layout::Unified => PaneRows::Unified(base),
                Layout::Split => PaneRows::Split(split_rows(&base)),
            }
        });
        Ok(match pane {
            PaneRows::Unified(rows) => RowsPage {
                total: rows.len(),
                slice: RowsSlice::Unified(page_of(rows, start, count, view_dto::row_dto)),
            },
            PaneRows::Split(rows) => RowsPage {
                total: rows.len(),
                slice: RowsSlice::Split(page_of(rows, start, count, view_dto::split_row_dto)),
            },
        })
    }
}

/// Saturating slice: an out-of-range `start` yields an empty page, never a panic.
fn page_of<T, D>(rows: &[T], start: usize, count: usize, map: impl Fn(&T) -> D) -> Vec<D> {
    let start = start.min(rows.len());
    let end = start.saturating_add(count).min(rows.len());
    rows[start..end].iter().map(map).collect()
}

#[cfg(test)]
mod tests {
    use domain::diffs::{Cmd, FileDiff, Foot, LineOwners, View};

    use super::*;
    use crate::recipe::{RecipeOp, RecipeSource, RecipeTarget};

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(ToString::to_string).collect()
    }

    fn recipe(path: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(path.into()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed,
            },
        }
    }

    fn view(title: &str) -> View {
        View {
            repo_name: "gt".into(),
            repo_root: "/repos/gt".into(),
            branch: "feature".into(),
            upstream: "origin/main".into(),
            commits: vec![],
            files: vec![FileDiff {
                path: "src/a.rs".into(),
                added: 1,
                removed: 1,
                lines: lines(&["@@ -1,2 +1,2 @@", " keep", "-old", "+new"]),
                full_lines: None,
                commits: vec![],
                owners: LineOwners::default(),
            }],
            title: title.into(),
            cmd: Cmd {
                lead: String::new(),
                range: "origin/main..HEAD".into(),
                trail: String::new(),
            },
            commits_label: String::new(),
            foot: Foot {
                cmd: String::new(),
                note: String::new(),
            },
            theme: None,
        }
    }

    #[test]
    fn same_recipe_reuses_the_tab_and_replaces_content() {
        let tabs = RenderedTabs::default();
        let first = tabs.upsert(recipe("/repos/gt"), "b1".into(), view("one"));
        let second = tabs.upsert(recipe("/repos/gt"), "b2".into(), view("two"));

        assert_eq!(first, second);
        let meta = tabs.meta(first).expect("tab exists");
        assert_eq!((meta.title.as_str(), meta.batch_id.as_str()), ("two", "b2"));
    }

    #[test]
    fn distinct_recipes_get_distinct_tabs() {
        let tabs = RenderedTabs::default();
        let a = tabs.upsert(recipe("/repos/a"), "b1".into(), view("a"));
        let b = tabs.upsert(recipe("/repos/b"), "b1".into(), view("b"));
        assert_ne!(a, b);
    }

    #[test]
    fn rows_page_slices_unified_rows_with_the_total() {
        let tabs = RenderedTabs::default();
        let id = tabs.upsert(recipe("/repos/gt"), "b1".into(), view("t"));

        let page = tabs
            .rows_page(id, 0, Layout::Unified, false, 1, 2)
            .expect("page succeeds");

        assert_eq!(page.total, 4, "hunk + context + del + add");
        let RowsSlice::Unified(rows) = page.slice else {
            panic!("unified expected");
        };
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].kind, rows[0].text.as_str()), ("context", "keep"));
        assert_eq!((rows[1].kind, rows[1].text.as_str()), ("del", "old"));
    }

    #[test]
    fn rows_page_serves_split_rows_and_out_of_range_start_is_empty() {
        let tabs = RenderedTabs::default();
        let id = tabs.upsert(recipe("/repos/gt"), "b1".into(), view("t"));

        let page = tabs
            .rows_page(id, 0, Layout::Split, false, 0, 100)
            .expect("page succeeds");
        assert_eq!(page.total, 3, "hunk + context + one pair");

        let empty = tabs
            .rows_page(id, 0, Layout::Split, false, 50, 10)
            .expect("page succeeds");
        let RowsSlice::Split(rows) = empty.slice else {
            panic!("split expected");
        };
        assert!(rows.is_empty());
        assert_eq!(empty.total, 3);
    }

    #[test]
    fn unknown_tab_and_bad_file_index_are_errors() {
        let tabs = RenderedTabs::default();
        assert!(tabs.rows_page(99, 0, Layout::Unified, false, 0, 1).is_err());
        let id = tabs.upsert(recipe("/repos/gt"), "b1".into(), view("t"));
        assert!(tabs.rows_page(id, 5, Layout::Unified, false, 0, 1).is_err());
    }

    #[test]
    fn replace_view_swaps_content_and_close_removes() {
        let tabs = RenderedTabs::default();
        let id = tabs.upsert(recipe("/repos/gt"), "b1".into(), view("old"));

        assert!(tabs.replace_view(id, view("new")));
        assert_eq!(tabs.meta(id).expect("tab exists").title, "new");

        assert!(tabs.close(id));
        assert!(!tabs.close(id), "second close is a no-op");
        assert!(tabs.meta(id).is_none());
    }
}
