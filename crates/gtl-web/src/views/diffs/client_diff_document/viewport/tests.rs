use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use dioxus::dioxus_core::NoOpMutations;
use gtl_models::diffs::DiffLineCount;
use gtl_wire::viewer::{
    ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout, ViewerFileStatus, ViewerFileSummary,
    ViewerRenderOptions,
};

use super::*;
use crate::{
    entities::diffs::{ClientDiffFileState, ClientDiffRows, ViewerUnifiedRow},
    test_support::{
        TestResult, absolute_file_path, repository_relative_path, unified_source_row, viewer_tab_id,
    },
};

const WINDOW_COUNT: usize = 3;
const ROW_COUNT: usize = WINDOW_COUNT * geometry::ROWS_PER_WINDOW;

#[derive(Clone, Default)]
struct RowWindowRenders(Rc<RefCell<BTreeMap<usize, usize>>>);

pub(super) fn record_row_window_render(batch: usize) {
    if let Some(renders) = try_consume_context::<RowWindowRenders>() {
        *renders.0.borrow_mut().entry(batch).or_default() += 1;
    }
}

fn loaded_file() -> TestResult<ClientDiffFile> {
    let rows = (0..WINDOW_COUNT)
        .map(|window| {
            (0..geometry::ROWS_PER_WINDOW)
                .map(|row| {
                    let line = u32::try_from(window * geometry::ROWS_PER_WINDOW + row + 1)?;
                    Ok(ViewerUnifiedRow::Added(unified_source_row(
                        &format!("window {window} row {row}"),
                        None,
                        Some(line),
                        None,
                    )))
                })
                .collect::<TestResult<Vec<_>>>()
        })
        .collect::<TestResult<Vec<_>>>()?;
    Ok(ClientDiffFile {
        summary: ViewerFileSummary {
            review: None,
            source_id: None,
            id: ViewerDiffFileId::for_index(0),
            path: repository_relative_path("src/rows.rs")?,
            absolute_path: absolute_file_path("/repo/src/rows.rs")?,
            anchor_id: "f-src-rows-rs".to_owned(),
            added: DiffLineCount::new(u64::try_from(ROW_COUNT)?),
            removed: DiffLineCount::default(),
            status: ViewerFileStatus::Added,
            can_open_in_editor: true,
            initially_expanded: true,
            row_count: ROW_COUNT,
        },
        rows: ClientDiffRows {
            unified: rows,
            split: Vec::new(),
        },
        line_number_digits: 3,
        state: ClientDiffFileState::Complete,
    })
}

#[component]
fn MountedRowWindows(file: ClientDiffFile, identity: ViewerViewIdentity) -> Element {
    let workspace = use_store(move || ClientDiffWorkspace {
        identity,
        files: vec![file],
    });
    let geometry = use_signal(|| DiffGeometry::new([(ROW_COUNT, true)].into_iter()));
    let browser = browser::use_viewport_browser(0.0);
    use_context_provider(RowWindowRenders::default);
    use_context_provider(|| ViewportContext {
        geometry,
        browser,
        presentation: DiffPresentation::detached(),
        workspace: workspace.into(),
        identity,
        content_id: ViewerRowContentId::from_digest([7; 32]),
    });
    let window = use_signal(|| geometry.peek().visible(0.0, 1_400.0).remove(0));
    use_context_provider(|| window);
    let folded = use_signal(|| None::<bool>);
    let flashing_file = use_signal(|| None::<String>);
    rsx! {
        ViewportFile {
            window: window(),
            folded,
            flashing_file,
            onopen: None,
            onretry: move |_| {},
            retry_allowed: false,
        }
    }
}

#[test]
fn mounting_a_row_window_leaves_mounted_windows_unrendered() -> TestResult {
    let identity = ViewerViewIdentity {
        tab_id: viewer_tab_id(1)?,
        range_generation: gtl_models::viewer::ViewerRangeGeneration::default(),
        selection_generation: gtl_models::viewer::ViewerSelectionGeneration::default(),
        render_options: ViewerRenderOptions {
            wrap_lines: false,
            layout: ViewerDiffLayout::Unified,
            density: ViewerDiffDensity::Compact,
        },
    };
    let mut dom = VirtualDom::new_with_props(
        MountedRowWindows,
        MountedRowWindowsProps {
            file: loaded_file()?,
            identity,
        },
    );
    dom.rebuild_in_place();
    let renders = dom
        .runtime()
        .consume_context::<RowWindowRenders>(ScopeId::APP)
        .ok_or("row window render counts")?;
    let mut window = dom
        .runtime()
        .consume_context::<Signal<FileWindow>>(ScopeId::APP)
        .ok_or("mounted file window")?;
    let geometry = dom
        .runtime()
        .consume_context::<ViewportContext>(ScopeId::APP)
        .ok_or("viewport context")?
        .geometry;
    assert_eq!(*renders.0.borrow(), BTreeMap::from([(0, 1), (1, 1)]));
    assert!(!dioxus_ssr::render(&dom).contains("window 2 row 0"));

    window.set(geometry.peek().visible(0.0, 2_800.0).remove(0));
    dom.render_immediate(&mut NoOpMutations);

    assert_eq!(
        *renders.0.borrow(),
        BTreeMap::from([(0, 1), (1, 1), (2, 1)])
    );
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("window 0 row 0"));
    assert!(html.contains("window 2 row 63"));
    Ok(())
}
