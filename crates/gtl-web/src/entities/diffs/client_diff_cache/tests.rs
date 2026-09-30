use std::collections::{HashMap, VecDeque};

use dioxus::prelude::*;
use gtl_models::{
    diffs::DiffLineCount,
    git::{BranchName, GitHead, GitRevision},
    viewer::{ViewerRangeGeneration, ViewerSelectionGeneration},
};
use gtl_wire::viewer::{
    ViewerActiveView, ViewerCommandLine, ViewerCommitSelection, ViewerDiffDensity,
    ViewerDiffFileId, ViewerDiffLayout, ViewerFileStatus, ViewerFileSummary, ViewerFooter,
    ViewerRenderOptions, ViewerRowContentId, ViewerUnifiedRow, ViewerViewIdentity,
};

use super::ClientDiffCache;
use crate::{
    entities::diffs::{ClientDiffFileState, ClientDiffWorkspace},
    test_support::{
        TestResult, absolute_file_path, project_name, repository_relative_path, viewer_tab_id,
    },
};

#[test]
fn same_content_reuses_row_allocation_and_store_across_tabs_and_generations() -> TestResult {
    let mut view = view(1)?;
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || -> TestResult {
        let cache = cache();
        let original = cache.select(Some("server-a".to_owned()), &view);
        complete(cache, &view);
        let rows = original.peek().files[0].rows.unified[0].as_ptr();

        let identities = [
            ViewerViewIdentity {
                tab_id: viewer_tab_id(2)?,
                ..view.identity
            },
            ViewerViewIdentity {
                range_generation: ViewerRangeGeneration::new(2),
                ..view.identity
            },
            ViewerViewIdentity {
                selection_generation: ViewerSelectionGeneration::new(2),
                ..view.identity
            },
        ];
        for (index, identity) in identities.into_iter().enumerate() {
            view.identity = identity;
            view.files[0].anchor_id = format!("selected-file-{index}");
            let mut selected = cache.select(Some("server-a".to_owned()), &view);

            assert_eq!(selected.peek().identity, identity);
            assert_eq!(selected.peek().files[0].summary, view.files[0]);
            assert_eq!(
                selected.peek().files[0].state,
                ClientDiffFileState::Complete
            );
            assert_eq!(selected.peek().files[0].rows.unified[0].as_ptr(), rows);
            selected.write().files[0].line_number_digits += 1;
            assert_eq!(original.peek().identity, identity);
            assert_eq!(
                original.peek().files[0].line_number_digits,
                selected.peek().files[0].line_number_digits
            );
        }
        assert_eq!(cache.workspaces.peek().len(), 1);
        assert_eq!(cache.recency.peek().len(), 1);
        Ok(())
    })
}

#[test]
fn changed_content_loads_fresh_rows_and_preserves_the_previous_content() -> TestResult {
    let original_view = view(1)?;
    let changed_view = view(2)?;
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || {
        let cache = cache();
        let original = cache.select(Some("server-a".to_owned()), &original_view);
        complete(cache, &original_view);
        let rows = original.peek().files[0].rows.unified[0].as_ptr();

        let changed = cache.select(Some("server-a".to_owned()), &changed_view);
        assert_loading(changed, &changed_view);
        assert_eq!(
            original.peek().files[0].state,
            ClientDiffFileState::Complete
        );

        let selected_again = cache.select(Some("server-a".to_owned()), &original_view);
        assert_eq!(
            selected_again.peek().files[0].rows.unified[0].as_ptr(),
            rows
        );
        assert_eq!(
            selected_again.peek().files[0].state,
            ClientDiffFileState::Complete
        );
    });
    Ok(())
}

#[test]
fn replacement_server_discards_previous_workspaces_and_recency() -> TestResult {
    let views = (1..=2).map(view).collect::<TestResult<Vec<_>>>()?;
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || {
        let cache = cache();
        for view in &views {
            complete(cache, view);
        }
        assert_eq!(cache.workspaces.peek().len(), 2);
        assert_eq!(cache.recency.peek().len(), 2);

        for server_instance_id in [Some("server-b".to_owned()), None] {
            let replacement = cache.select(server_instance_id.clone(), &views[0]);
            assert_loading(replacement, &views[0]);
            assert_eq!(cache.workspaces.peek().len(), 1);
            assert_eq!(cache.recency.peek().len(), 1);
            assert!(
                cache
                    .workspaces
                    .peek()
                    .keys()
                    .all(|key| key.server_instance_id == server_instance_id)
            );
            assert!(
                cache
                    .recency
                    .peek()
                    .iter()
                    .all(|key| key.server_instance_id == server_instance_id)
            );
            complete_on_server(cache, &views[0], server_instance_id);
        }
    });
    Ok(())
}

#[test]
fn ninth_content_evicts_the_least_recently_selected_workspace() -> TestResult {
    let views = (1..=9).map(view).collect::<TestResult<Vec<_>>>()?;
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || {
        let cache = cache();
        for view in &views[..8] {
            complete(cache, view);
        }
        assert_eq!(cache.workspaces.peek().len(), 8);

        let recently_selected = cache.select(Some("server-a".to_owned()), &views[0]);
        let rows = recently_selected.peek().files[0].rows.unified[0].as_ptr();
        complete(cache, &views[8]);

        assert_eq!(cache.workspaces.peek().len(), 8);
        assert_eq!(cache.recency.peek().len(), 8);
        let retained = cache.select(Some("server-a".to_owned()), &views[0]);
        assert_eq!(retained.peek().files[0].rows.unified[0].as_ptr(), rows);
        assert_eq!(
            retained.peek().files[0].state,
            ClientDiffFileState::Complete
        );
        let newest = cache.select(Some("server-a".to_owned()), &views[8]);
        assert_eq!(newest.peek().files[0].state, ClientDiffFileState::Complete);

        let evicted = cache.select(Some("server-a".to_owned()), &views[1]);
        assert_loading(evicted, &views[1]);
        assert_eq!(cache.workspaces.peek().len(), 8);
        assert_eq!(cache.recency.peek().len(), 8);
    });
    Ok(())
}

#[test]
fn active_and_inactive_windows_share_the_same_byte_bound() -> TestResult {
    let views = (1..=2).map(view).collect::<TestResult<Vec<_>>>()?;
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || {
        let cache = cache();
        let first = cache.select(Some("server-a".to_owned()), &views[0]);
        complete_with_capacity(cache, &views[0], 33 * 1024 * 1024).unwrap();
        let second = cache.select(Some("server-a".to_owned()), &views[1]);
        complete_with_capacity(cache, &views[1], 33 * 1024 * 1024).unwrap();
        assert!(first.peek().files[0].rows.unified[0].is_empty());
        assert!(!second.peek().files[0].rows.unified[0].is_empty());
        assert!(cache.rows.peek().bytes <= super::RETAINED_ROW_BYTES_MAX);
        assert_eq!(cache.workspaces.peek().len(), 2);
        assert_eq!(first.peek().files[0].summary, views[0].files[0]);
    });
    Ok(())
}

#[test]
fn a_single_active_file_evicts_old_windows_without_losing_file_metadata() -> TestResult {
    let mut view = view(1)?;
    view.files[0].row_count = 128;
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || {
        let cache = cache();
        let active = cache.select(Some("server-a".to_owned()), &view);
        for batch in 0..2 {
            let mut rows = vec![ViewerUnifiedRow::Meta(String::new()); 64];
            rows[0] = ViewerUnifiedRow::Meta(String::with_capacity(33 * 1024 * 1024));
            cache
                .retain_window(
                    &key(&view, Some("server-a".to_owned())),
                    super::ClientDiffWindow { file: 0, batch },
                    super::LoadedRowWindow {
                        rows: gtl_wire::viewer::ViewerRows::Unified(rows),
                        line_number_digits: 3,
                    },
                    &[],
                )
                .unwrap();
        }
        assert!(active.peek().files[0].rows.unified[0].is_empty());
        assert_eq!(active.peek().files[0].rows.unified[1].len(), 64);
        let mut rows = vec![ViewerUnifiedRow::Meta(String::new()); 64];
        rows[0] = ViewerUnifiedRow::Meta(String::with_capacity(33 * 1024 * 1024));
        assert!(
            !cache
                .retain_window(
                    &key(&view, Some("server-a".to_owned())),
                    super::ClientDiffWindow { file: 0, batch: 0 },
                    super::LoadedRowWindow {
                        rows: gtl_wire::viewer::ViewerRows::Unified(rows),
                        line_number_digits: 3
                    },
                    &[super::ClientDiffWindow { file: 0, batch: 1 }],
                )
                .unwrap()
        );
        assert!(active.peek().files[0].rows.unified[0].is_empty());
        assert_eq!(active.peek().files[0].rows.unified[1].len(), 64);
        assert_eq!(active.peek().files[0].summary, view.files[0]);
        assert!(cache.rows.peek().bytes <= super::RETAINED_ROW_BYTES_MAX);
    });
    Ok(())
}

#[test]
fn an_oversize_window_is_rejected_without_evicting_existing_rows() -> TestResult {
    let view = view(1)?;
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || {
        let cache = cache();
        let active = cache.select(Some("server-a".to_owned()), &view);
        complete(cache, &view);
        let pointer = active.peek().files[0].rows.unified[0].as_ptr();
        assert!(complete_with_capacity(cache, &view, 65 * 1024 * 1024).is_err());
        assert_eq!(active.peek().files[0].rows.unified[0].as_ptr(), pointer);
        assert!(cache.rows.peek().bytes <= super::RETAINED_ROW_BYTES_MAX);
    });
    Ok(())
}

fn key(view: &ViewerActiveView, server_instance_id: Option<String>) -> super::ClientDiffCacheKey {
    super::ClientDiffCacheKey {
        server_instance_id,
        content_id: view.content_id,
    }
}

fn complete_with_capacity(
    cache: ClientDiffCache,
    view: &ViewerActiveView,
    capacity: usize,
) -> Result<(), super::ClientDiffFileError> {
    let mut text = String::with_capacity(capacity);
    text.push_str("cached");
    cache
        .retain_window(
            &key(view, Some("server-a".to_owned())),
            super::ClientDiffWindow { file: 0, batch: 0 },
            super::LoadedRowWindow {
                rows: gtl_wire::viewer::ViewerRows::Unified(vec![ViewerUnifiedRow::Meta(text)]),
                line_number_digits: 1,
            },
            &[],
        )
        .map(|_| ())
}

fn cache() -> ClientDiffCache {
    ClientDiffCache {
        workspaces: Store::new(HashMap::new()),
        recency: Signal::new(VecDeque::new()),
        rows: Signal::new(super::RetainedRows::default()),
        requests: Signal::new(VecDeque::new()),
    }
}

fn complete(cache: ClientDiffCache, view: &ViewerActiveView) {
    complete_on_server(cache, view, Some("server-a".to_owned()));
}

fn complete_on_server(cache: ClientDiffCache, view: &ViewerActiveView, server: Option<String>) {
    let mut workspace = cache.select(server.clone(), view);
    cache
        .retain_window(
            &key(view, server),
            super::ClientDiffWindow { file: 0, batch: 0 },
            super::LoadedRowWindow {
                rows: gtl_wire::viewer::ViewerRows::Unified(vec![ViewerUnifiedRow::Meta(
                    "cached".to_owned(),
                )]),
                line_number_digits: 1,
            },
            &[],
        )
        .unwrap();
    workspace.write().files[0].state = ClientDiffFileState::Complete;
}

fn assert_loading(workspace: Store<ClientDiffWorkspace>, view: &ViewerActiveView) {
    let workspace = workspace.peek();
    assert_eq!(workspace.identity, view.identity);
    assert_eq!(workspace.files.len(), view.files.len());
    let file = &workspace.files[0];
    assert_eq!(file.summary, view.files[0]);
    assert_eq!(file.state, ClientDiffFileState::Loading);
    assert!(file.rows.unified.iter().all(Vec::is_empty));
    assert!(file.rows.split.is_empty());
}

fn view(content: u8) -> TestResult<ViewerActiveView> {
    Ok(ViewerActiveView {
        modified_files: false,
        row_source: gtl_wire::viewer::ViewerRowSourceState::Ready,
        identity: ViewerViewIdentity {
            tab_id: viewer_tab_id(1)?,
            range_generation: ViewerRangeGeneration::new(1),
            selection_generation: ViewerSelectionGeneration::default(),
            render_options: ViewerRenderOptions {
                wrap_lines: false,
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        },
        content_id: ViewerRowContentId::from_digest([content; 32]),
        title: gtl_models::diffs::DiffViewTitle::Diff,
        repository_name: project_name("repo")?,
        branch: GitHead::Branch(BranchName::main()),
        upstream: GitRevision::main(),
        command: ViewerCommandLine {
            lead: "git diff ".to_owned(),
            range: "HEAD".to_owned(),
            trail: String::new(),
        },
        files: vec![ViewerFileSummary {
            source_id: None,
            id: ViewerDiffFileId::for_index(0),
            path: repository_relative_path("src/main.rs")?,
            absolute_path: absolute_file_path("/repo/src/main.rs")?,
            anchor_id: "file-0".to_owned(),
            added: DiffLineCount::new(1),
            removed: DiffLineCount::default(),
            status: ViewerFileStatus::Added,
            can_open_in_editor: true,
            initially_expanded: true,
            row_count: 1,
        }],
        commit_count: 0,
        commits: Vec::new(),
        commit_selection: ViewerCommitSelection::None,
        footer: ViewerFooter {
            command: "git diff HEAD".to_owned(),
        },
        extension_filter: None,
        changes_since: None,
    })
}

#[test]
fn extension_changes_reuse_unchanged_file_windows_after_indices_move() -> TestResult {
    let mut original = view(1)?;
    original.files[0].source_id = Some(ViewerRowContentId::from_digest([11; 32]));
    let mut expanded = original.clone();
    expanded.content_id = ViewerRowContentId::from_digest([2; 32]);
    let mut revealed = original.files[0].clone();
    revealed.path = repository_relative_path("Cargo.lock")?;
    revealed.source_id = Some(ViewerRowContentId::from_digest([22; 32]));
    expanded.files.insert(0, revealed);
    expanded.files[1].id = ViewerDiffFileId::for_index(1);
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || {
        let cache = cache();
        complete(cache, &original);
        let selected = cache.select(Some("server-a".to_owned()), &expanded);
        let workspace = selected.peek();
        assert!(workspace.files[0].rows.unified[0].is_empty());
        assert_eq!(
            workspace.files[1].rows.unified[0],
            vec![ViewerUnifiedRow::Meta("cached".to_owned())]
        );
        assert_eq!(workspace.files[1].state, ClientDiffFileState::Complete);
        assert!(cache.rows.peek().bytes <= super::RETAINED_ROW_BYTES_MAX);
    });
    Ok(())
}

#[test]
fn same_path_and_counts_do_not_reuse_changed_file_sources() -> TestResult {
    let mut original = view(1)?;
    original.files[0].source_id = Some(ViewerRowContentId::from_digest([11; 32]));
    let mut changed = original.clone();
    changed.content_id = ViewerRowContentId::from_digest([2; 32]);
    changed.files[0].source_id = Some(ViewerRowContentId::from_digest([22; 32]));
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || {
        let cache = cache();
        complete(cache, &original);
        assert_loading(
            cache.select(Some("server-a".to_owned()), &changed),
            &changed,
        );
    });
    Ok(())
}
