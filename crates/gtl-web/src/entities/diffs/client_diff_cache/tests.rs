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
        TestResult, absolute_file_path, project_name, repository_relative_path, unified_source_row,
        viewer_tab_id,
    },
};

#[test]
fn same_content_reuses_row_allocation_and_store_across_tabs_and_generations() -> TestResult {
    let mut view = view(1)?;
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || -> TestResult {
        let cache = cache();
        let original = cache.select(Some("server-a".to_owned()), &view);
        complete(original);
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
        complete(original);
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
            complete(cache.select(Some("server-a".to_owned()), view));
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
            complete(replacement);
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
            complete(cache.select(Some("server-a".to_owned()), view));
        }
        assert_eq!(cache.workspaces.peek().len(), 8);

        let recently_selected = cache.select(Some("server-a".to_owned()), &views[0]);
        let rows = recently_selected.peek().files[0].rows.unified[0].as_ptr();
        complete(cache.select(Some("server-a".to_owned()), &views[8]));

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
fn retained_row_capacity_over_64_mib_evicts_oldest_content_below_the_entry_limit() -> TestResult {
    let views = (1..=3).map(view).collect::<TestResult<Vec<_>>>()?;
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || {
        let cache = cache();
        complete_with_capacity(
            cache.select(Some("server-a".to_owned()), &views[0]),
            33 * 1024 * 1024,
        );
        let retained = cache.select(Some("server-a".to_owned()), &views[1]);
        complete_with_capacity(retained, 33 * 1024 * 1024);
        let rows = retained.peek().files[0].rows.unified[0].as_ptr();

        let active = cache.select(Some("server-a".to_owned()), &views[2]);
        assert_loading(active, &views[2]);
        assert_eq!(cache.workspaces.peek().len(), 2);
        assert_eq!(cache.recency.peek().len(), 2);
        let selected_again = cache.select(Some("server-a".to_owned()), &views[1]);
        assert_eq!(
            selected_again.peek().files[0].rows.unified[0].as_ptr(),
            rows
        );
        assert_eq!(
            selected_again.peek().files[0].state,
            ClientDiffFileState::Complete
        );

        let evicted = cache.select(Some("server-a".to_owned()), &views[0]);
        assert_loading(evicted, &views[0]);
    });
    Ok(())
}

#[test]
fn oversized_active_workspace_survives_trim_until_another_content_is_selected() -> TestResult {
    let large_view = view(1)?;
    let next_view = view(2)?;
    let owner = VirtualDom::new(VNode::empty);
    owner.in_scope(ScopeId::ROOT, || {
        let cache = cache();
        let active = cache.select(Some("server-a".to_owned()), &large_view);
        complete_with_capacity(active, 65 * 1024 * 1024);
        let rows = active.peek().files[0].rows.unified[0].as_ptr();

        cache.trim();
        assert_eq!(cache.workspaces.peek().len(), 1);
        let selected_again = cache.select(Some("server-a".to_owned()), &large_view);
        assert_eq!(
            selected_again.peek().files[0].rows.unified[0].as_ptr(),
            rows
        );
        assert_eq!(
            selected_again.peek().files[0].state,
            ClientDiffFileState::Complete
        );

        let next = cache.select(Some("server-a".to_owned()), &next_view);
        assert_loading(next, &next_view);
        assert_eq!(cache.workspaces.peek().len(), 1);
        assert_eq!(cache.recency.peek().len(), 1);
        let evicted = cache.select(Some("server-a".to_owned()), &large_view);
        assert_loading(evicted, &large_view);
    });
    Ok(())
}

fn complete_with_capacity(mut workspace: Store<ClientDiffWorkspace>, capacity: usize) {
    let mut text = String::with_capacity(capacity);
    text.push_str("cached");
    let mut workspace = workspace.write();
    workspace.files[0].rows.unified = vec![vec![ViewerUnifiedRow::Meta(text)]];
    workspace.files[0].state = ClientDiffFileState::Complete;
}

fn cache() -> ClientDiffCache {
    ClientDiffCache {
        workspaces: Store::new(HashMap::new()),
        recency: Signal::new(VecDeque::new()),
    }
}

fn complete(mut workspace: Store<ClientDiffWorkspace>) {
    let mut workspace = workspace.write();
    let file = &mut workspace.files[0];
    file.rows.unified = vec![vec![ViewerUnifiedRow::Added(unified_source_row(
        "let value = 42;",
        None,
        Some(1),
        None,
    ))]];
    file.state = ClientDiffFileState::Complete;
}

fn assert_loading(workspace: Store<ClientDiffWorkspace>, view: &ViewerActiveView) {
    let workspace = workspace.peek();
    assert_eq!(workspace.identity, view.identity);
    assert_eq!(workspace.files.len(), view.files.len());
    let file = &workspace.files[0];
    assert_eq!(file.summary, view.files[0]);
    assert_eq!(file.state, ClientDiffFileState::Loading);
    assert!(file.rows.unified.is_empty());
    assert!(file.rows.split.is_empty());
}

fn view(content: u8) -> TestResult<ViewerActiveView> {
    Ok(ViewerActiveView {
        identity: ViewerViewIdentity {
            tab_id: viewer_tab_id(1)?,
            range_generation: ViewerRangeGeneration::new(1),
            selection_generation: ViewerSelectionGeneration::default(),
            render_options: ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        },
        content_id: ViewerRowContentId::from_digest([content; 32]),
        title: "Working tree".to_owned(),
        repository_name: project_name("repo")?,
        branch: GitHead::Branch(BranchName::main()),
        upstream: GitRevision::main(),
        command: ViewerCommandLine {
            lead: "git diff ".to_owned(),
            range: "HEAD".to_owned(),
            trail: String::new(),
        },
        files: vec![ViewerFileSummary {
            id: ViewerDiffFileId::for_index(0),
            path: repository_relative_path("src/main.rs")?,
            absolute_path: absolute_file_path("/repo/src/main.rs")?,
            anchor_id: "file-0".to_owned(),
            added: DiffLineCount::new(1),
            removed: DiffLineCount::default(),
            status: ViewerFileStatus::Added,
            can_open_in_editor: true,
            initially_expanded: true,
        }],
        commits_label: "0 commits".to_owned(),
        commit_count: 0,
        commits: Vec::new(),
        commit_selection: ViewerCommitSelection::None,
        footer: ViewerFooter {
            command: "git diff HEAD".to_owned(),
        },
        exclusions: None,
    })
}
