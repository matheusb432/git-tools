use crate::shared::{
    i18n::{t, use_language},
    ui::ScrollArea,
};
mod browser;
pub(in crate::views::diffs) mod geometry;
#[cfg(test)]
mod tests;

use dioxus::prelude::*;
use gtl_wire::viewer::{
    ViewerDiffFileId, ViewerDiffLayout, ViewerRowContentId, ViewerViewIdentity,
};

use self::{
    browser::ViewportBrowser,
    geometry::{DiffGeometry, FileWindow},
};
use super::{
    file::{DiffFileCard, DiffFileControls},
    scroll_area::DiffRowsScrollArea,
};
use crate::{
    entities::diffs::{
        ClientDiffFile, ClientDiffFileStoreExt, ClientDiffRowsStoreExt, ClientDiffWindow,
        ClientDiffWorkspace, ClientDiffWorkspaceController, ClientDiffWorkspaceStoreExt,
    },
    views::diffs::{
        SplitDiffRowBatch, UnifiedDiffRowBatch,
        presentation::{DiffPresentation, ScrollAnchor},
    },
};

#[derive(Clone, Copy, PartialEq)]
struct ViewportContext {
    geometry: Signal<DiffGeometry>,
    browser: ViewportBrowser,
    presentation: DiffPresentation,
    workspace: ReadStore<ClientDiffWorkspace>,
    identity: ViewerViewIdentity,
    content_id: ViewerRowContentId,
}

impl ViewportContext {
    fn set_expanded(mut self, file: usize, expanded: bool) {
        if self.geometry.peek().expanded(file) == Some(expanded) {
            return;
        }
        let path = self.workspace.peek().files[file]
            .summary
            .path
            .to_string_lossy()
            .into_owned();
        self.presentation
            .set_file_expanded(self.identity.tab_id, path, expanded);
        self.geometry.write().set_expanded(file, expanded);
    }

    fn measure_header(mut self, file: usize, event: &ResizeEvent) {
        let Ok(size) = event.data().get_border_box_size() else {
            return;
        };
        if !measurement_changed(self.geometry.peek().header_height(file), size.height) {
            return;
        }
        if self.geometry.write().measure_header(file, size.height) {
            self.restore_anchor();
        }
    }

    fn measure_window(mut self, file: usize, window: usize, event: &ResizeEvent) {
        let Ok(size) = event.data().get_border_box_size() else {
            return;
        };
        if !measurement_changed(
            self.geometry.peek().window_height(file, window),
            size.height,
        ) {
            self.browser.refresh.call(());
            return;
        }
        if self
            .geometry
            .write()
            .measure_window(file, window, size.height)
        {
            self.restore_anchor();
        } else {
            self.browser.refresh.call(());
        }
    }

    fn restore_anchor(self) {
        if let Some(anchor) = self.presentation.anchor(self.identity.tab_id) {
            self.browser.restore(anchor);
        }
    }

    fn jump(self, anchor: ScrollAnchor) {
        let Some(file) = self
            .workspace
            .peek()
            .files
            .iter()
            .position(|file| file.summary.path.to_string_lossy() == anchor.file)
        else {
            return;
        };
        self.set_expanded(file, true);
        let geometry = self.geometry.peek();
        let top = anchor
            .row
            .and_then(|row| geometry.row_offset(file, row as usize))
            .unwrap_or_else(|| geometry.file_offset(file))
            + anchor.offset;
        self.presentation
            .set_anchor(self.identity.tab_id, anchor.clone());
        self.browser.scroll_to(top);
        self.browser.restore(anchor);
    }
}

fn measurement_changed(previous: Option<f64>, height: f64) -> bool {
    height.is_finite()
        && height > 0.0
        && previous.is_some_and(|previous| (previous - height).abs() >= 0.25)
}

#[component]
pub(super) fn DiffViewport(
    title: String,
    workspace: ReadStore<ClientDiffWorkspace>,
    identity: ViewerViewIdentity,
    content_id: ViewerRowContentId,
    controller: ClientDiffWorkspaceController,
    search_target: ReadSignal<Option<super::DiffSearchTarget>>,
    folded: ReadSignal<Option<bool>>,
    fold_command: ReadSignal<Option<crate::views::diffs::diff_workspace::FileFoldCommand>>,
    flashing_file: ReadSignal<Option<String>>,
    is_loading: bool,
    retry_allowed: bool,
    onopen: Option<EventHandler<ViewerDiffFileId>>,
    onretry: EventHandler<ViewerDiffFileId>,
) -> Element {
    let presentation = use_context::<DiffPresentation>();
    let (mut geometry, mut retained_width) = use_hook(move || {
        presentation.ensure_tab(identity.tab_id);
        let (width, geometry) = presentation
            .take_geometry(
                identity.tab_id,
                content_id,
                identity.render_options.wrap_lines,
            )
            .unwrap_or_else(|| (0.0, make_geometry(workspace, presentation, identity)));
        (Signal::new(geometry), Signal::new(width))
    });
    let initial_anchor = use_hook(move || presentation.anchor(identity.tab_id));
    let initial_top = use_hook(move || {
        anchor_offset(&geometry.peek(), &workspace.peek(), initial_anchor.as_ref())
    });
    let browser = browser::use_viewport_browser(initial_top);
    let context = ViewportContext {
        geometry,
        browser,
        presentation,
        workspace,
        identity,
        content_id,
    };
    use_context_provider(|| context);
    use_effect(move || {
        let target = search_target();
        let target = target
            .filter(|target| target.identity == identity)
            .and_then(|target| {
                let workspace = workspace.peek();
                let (index, file) = workspace
                    .files
                    .iter()
                    .enumerate()
                    .find(|(_, file)| file.summary.id == target.file)?;
                Some((
                    index,
                    target.row,
                    file.summary.path.to_string_lossy().into_owned(),
                ))
            });
        context
            .browser
            .search(target.as_ref().map(|(index, row, _)| (*index, *row)));
        let Some((_, row, file)) = target else { return };
        let Ok(row) = u32::try_from(row) else {
            return;
        };
        context.jump(ScrollAnchor {
            file,
            row: Some(row),
            offset: -40.0,
        });
    });
    let mut folded_initialized = use_signal(|| false);
    use_effect(move || {
        let command = fold_command();
        if !*folded_initialized.peek() {
            folded_initialized.set(true);
            return;
        }
        if let Some(command) = command.filter(|command| command.tab_id == identity.tab_id) {
            let folded = command.folded;
            let mut geometry = geometry.write();
            for file in 0..workspace.peek().files.len() {
                geometry.set_expanded(file, !folded);
            }
            if folded {
                context.browser.scroll_to(0.0);
            }
        }
    });
    use_effect(move || {
        let anchor = flashing_file();
        let Some(anchor) = anchor else { return };
        let path = workspace
            .peek()
            .files
            .iter()
            .find(|file| file.summary.anchor_id == anchor)
            .map(|file| file.summary.path.to_string_lossy().into_owned());
        if let Some(file) = path {
            context.jump(ScrollAnchor {
                file,
                row: None,
                offset: 0.0,
            });
        }
    });
    use_effect(move || {
        let metrics = browser.metrics.read();
        if metrics.width > 0.0 {
            let previous = *retained_width.peek();
            if previous > 0.0 && (metrics.width - previous).abs() > 1.0 {
                geometry.set(make_geometry(workspace, presentation, identity));
                context.restore_anchor();
            }
            if (*retained_width.peek() - metrics.width).abs() > 0.25 {
                retained_width.set(metrics.width);
            }
        }
        if let Some(anchor) = &metrics.anchor {
            presentation.set_anchor(identity.tab_id, anchor.clone());
        }
    });
    use_drop(move || {
        presentation.keep_geometry(
            identity.tab_id,
            content_id,
            identity.render_options.wrap_lines,
            *retained_width.peek(),
            geometry.peek().clone(),
        );
    });
    let visible = use_memo(move || {
        let metrics = browser.metrics.read();
        geometry.read().visible_with_pins(
            (metrics.top - metrics.height / 2.0).max(0.0),
            metrics.height * 2.0,
            &metrics.pins,
        )
    });
    let requested = use_memo(move || {
        let metrics = browser.metrics.read();
        let geometry = geometry.read();
        let viewport = geometry.visible(metrics.top, metrics.height);
        let nearby = visible();
        let mut windows = Vec::new();
        for file in viewport.into_iter().chain(nearby) {
            for placement in file.windows {
                let window = ClientDiffWindow {
                    file: file.file,
                    batch: placement.index,
                };
                if !windows.contains(&window) {
                    windows.push(window);
                }
            }
        }
        windows
    });
    use_effect(move || controller.request_windows(identity, requested()));

    let visible = visible();
    let geometry = geometry.read();
    let after = visible.last().map_or(0.0, |file| {
        geometry.total_height() - geometry.file_offset(file.file + 1)
    });
    let file_count = workspace.peek().files.len();
    let row_count = workspace
        .peek()
        .files
        .iter()
        .map(|file| file.summary.row_count)
        .sum::<usize>();
    rsx! {
        ScrollArea {
            class: "diff-document-scroll h-full min-h-0",
            style: "overflow-anchor: none;",
            role: "region",
            aria_label: t!(use_language(), "diff-rendered-for", title = title.as_str()),
            aria_busy: is_loading.to_string(),
            "data-gtl-diff-document": "",
            "data-wrap-lines": identity.render_options.wrap_lines.to_string(),
            "data-view-state": if is_loading { "streaming" } else { "complete" },
            "data-chunks-complete": (!is_loading).to_string(),
            "data-view-identity": format!(
                "{}:{}:{}:{}:{}",
                identity.tab_id,
                identity.range_generation.value(),
                identity.selection_generation.value(),
                identity.render_options.layout.as_str(),
                identity.render_options.density.as_str(),
            ),
            "data-layout": identity.render_options.layout.as_str(),
            "data-density": identity.render_options.density.as_str(),
            "data-total-files": file_count.to_string(),
            "data-total-rows": row_count.to_string(),
            onmounted: move |event| {
                browser.mount(&event);
                browser.scroll_to(initial_top);
                context.restore_anchor();
            },
            onscroll: move |_| browser.refresh.call(()),
            onresize: move |_| browser.refresh.call(()),
            onfocusin: move |_| browser.refresh.call(()),
            onfocusout: move |_| browser.refresh.call(()),
            for window in visible {
                ViewportFile {
                    key: "{window.file}",
                    window,
                    folded,
                    flashing_file,
                    onopen,
                    onretry,
                    retry_allowed,
                }
            }
            div { style: "height: {after}px;", aria_hidden: "true" }
            div { class: "diff-document-clearance", aria_hidden: "true" }
        }
    }
}

fn make_geometry(
    workspace: ReadStore<ClientDiffWorkspace>,
    presentation: DiffPresentation,
    identity: ViewerViewIdentity,
) -> DiffGeometry {
    DiffGeometry::new(workspace.peek().files.iter().map(|file| {
        (
            file.summary.row_count,
            presentation.file_expanded(identity.tab_id, &file.summary.path.to_string_lossy(), true),
        )
    }))
}

fn anchor_offset(
    geometry: &DiffGeometry,
    workspace: &ClientDiffWorkspace,
    anchor: Option<&ScrollAnchor>,
) -> f64 {
    let Some(anchor) = anchor else { return 0.0 };
    let Some(file) = workspace
        .files
        .iter()
        .position(|file| file.summary.path.to_string_lossy() == anchor.file)
    else {
        return 0.0;
    };
    anchor
        .row
        .and_then(|row| geometry.row_offset(file, row as usize))
        .unwrap_or_else(|| geometry.file_offset(file))
        + anchor.offset
}

#[component]
fn ViewportFile(
    window: FileWindow,
    folded: ReadSignal<Option<bool>>,
    flashing_file: ReadSignal<Option<String>>,
    onopen: Option<EventHandler<ViewerDiffFileId>>,
    onretry: EventHandler<ViewerDiffFileId>,
    retry_allowed: bool,
) -> Element {
    let context = use_context::<ViewportContext>();
    let index = window.file;
    let Some(file) = context.workspace.files().get(index) else {
        return rsx! {};
    };
    let open = use_memo(move || context.geometry.read().expanded(index).unwrap_or(false));
    let controls = DiffFileControls {
        open: open.into(),
        onchange: use_callback(move |expanded| context.set_expanded(index, expanded)),
        onresize: use_callback(move |event| context.measure_header(index, &event)),
    };
    let onretry_file = use_callback(move |()| onretry.call(file.summary().peek().id.clone()));
    let layout = context.identity.render_options.layout;
    let digits = file.line_number_digits().cloned();
    let before_file = window.before_file;
    let after = window.after_rows;
    let body = rsx! {
        DiffRowsScrollArea {
            id: "viewer-diff-{index}",
            layout,
            density: context.identity.render_options.density,
            line_number_digits: digits,
            for placement in window.windows {
                div { key: "{placement.index}",
                    div {
                        style: "height: {placement.before}px;",
                        aria_hidden: "true",
                    }
                    ViewportRowWindow {
                        file_index: index,
                        batch: placement.index,
                        layout,
                        onretry: onretry_file,
                        retry_allowed,
                    }
                }
            }
            div { style: "height: {after}px;", aria_hidden: "true" }
        }
    };
    rsx! {
        div { style: "height: {before_file}px;", aria_hidden: "true" }
        DiffFileCard {
            file,
            layout,
            density: context.identity.render_options.density,
            folded,
            flashing_file,
            onopen,
            onretry: onretry_file,
            retry_allowed,
            file_index: index,

            controls: Some(controls),
            body: Some(body),
        }
    }
}

#[component]
fn ViewportRowWindow(
    file_index: usize,
    batch: usize,
    layout: ViewerDiffLayout,
    onretry: EventHandler<()>,
    retry_allowed: bool,
) -> Element {
    #[cfg(test)]
    tests::record_row_window_render(batch);
    let context = use_context::<ViewportContext>();
    let Some(file) = context.workspace.files().get(file_index) else {
        return rsx! {};
    };
    let file: ReadStore<ClientDiffFile> = file.into();
    let loaded = match layout {
        ViewerDiffLayout::Unified => file
            .rows()
            .unified()
            .get(batch)
            .is_some_and(|batch| !batch.is_empty()),
        ViewerDiffLayout::Split => file
            .rows()
            .split()
            .get(batch)
            .is_some_and(|batch| !batch.is_empty()),
    };
    let height = context
        .geometry
        .peek()
        .window_height(file_index, batch)
        .unwrap_or(0.0);
    rsx! {
        div {
            "data-gtl-row-window": batch.to_string(),
            style: (!loaded).then(|| format!("height: {height}px;")),
            aria_busy: (!loaded).to_string(),
            onresize: move |event| {
                if loaded {
                    context.measure_window(file_index, batch, &event);
                }
            },
            if !loaded {
                div { class: "sticky top-10 z-10",
                    super::file::rows::DiffFileLoadState { state: file.state(), retry_allowed, onretry }
                }
            }
            if loaded {
                if layout == ViewerDiffLayout::Unified {
                    UnifiedDiffRowBatch { file, batch_index: batch }
                } else {
                    SplitDiffRowBatch { file, batch_index: batch }
                }
            }
        }
    }
}
