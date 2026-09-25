use std::rc::Rc;

use dioxus::prelude::*;
use gtl_models::failure::Failure;
use gtl_wire::viewer::{
    ReadViewerDiffText, ViewerDiffTextLine, ViewerRowRange, ViewerViewIdentity,
};
use wasm_bindgen::{JsCast as _, closure::Closure};

use super::{ContextCopyStatus, ContextualizedCopy, SelectedDiffLines};
use crate::{
    entities::diffs::{ClientDiffWorkspace, viewer_server},
    shared::{
        browser,
        i18n::t,
        ui::{ToastHandle, ToastText, use_toast},
        viewer_client::ViewerClientError,
    },
};

mod cached;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Position {
    file: usize,
    row: usize,
}

#[derive(Clone, Copy)]
struct Selection {
    start: Position,
    end: Position,
    old_side: bool,
}

impl Selection {
    fn rows(self, file: usize, count: usize) -> std::ops::Range<usize> {
        let start = if file == self.start.file {
            self.start.row
        } else {
            0
        };
        let end = if file == self.end.file {
            self.end.row.saturating_add(1)
        } else {
            count
        };
        start..end.min(count)
    }
}

pub(in crate::views::diffs::client_diff_document) fn use_diff_copy(
    identity: ViewerViewIdentity,
    workspace: ReadStore<ClientDiffWorkspace>,
) {
    let toast = use_toast();
    let mut copy = use_action(move |selection: Selection| async move {
        complete_copy(read_text(identity, workspace, selection).await, toast).await;
        Ok::<(), std::convert::Infallible>(())
    });
    use_effect(use_reactive((&identity,), move |_| copy.cancel()));
    let defer = use_callback(move |selection| {
        copy.call(selection);
    });
    let callback = use_callback(move |event: web_sys::ClipboardEvent| {
        handle_copy(&event, workspace, toast, defer);
    });
    let _listener = dioxus::dioxus_core::use_hook_with_cleanup(
        move || {
            if !cfg!(target_arch = "wasm32") {
                return None;
            }
            let document = web_sys::window()?.document()?;
            let listener = Rc::new(Closure::wrap(Box::new(move |event| callback.call(event))
                as Box<dyn FnMut(web_sys::ClipboardEvent)>));
            document
                .add_event_listener_with_callback(
                    "copy",
                    listener.as_ref().as_ref().unchecked_ref(),
                )
                .ok()?;
            Some((document, listener))
        },
        |listener| {
            if let Some((document, listener)) = listener {
                let _ = document.remove_event_listener_with_callback(
                    "copy",
                    listener.as_ref().as_ref().unchecked_ref(),
                );
            }
        },
    );
}

async fn complete_copy(
    result: Result<Option<ContextualizedCopy>, ViewerClientError>,
    toast: ToastHandle,
) {
    match result {
        Ok(Some(text)) => {
            if browser::copy_text(&text.text).await {
                toast.ok(copy_status_toast(text.status));
            } else {
                toast.error(localized_toast(SelectionCopyMessage::Failed));
            }
        }
        Ok(None) => toast.info(localized_toast(SelectionCopyMessage::Empty)),
        Err(error) => toast.client_error(&error),
    }
}

fn handle_copy(
    event: &web_sys::ClipboardEvent,
    workspace: ReadStore<ClientDiffWorkspace>,
    toast: ToastHandle,
    defer: Callback<Selection>,
) {
    if event.default_prevented()
        || event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|target| {
                target
                    .closest("input, textarea, [contenteditable='true']")
                    .ok()
                    .flatten()
                    .is_some()
            })
    {
        return;
    }
    let Some(selection) = selection(&workspace.peek()) else {
        return;
    };
    match cached_text(&workspace.peek(), selection) {
        Ok(Some(text)) => {
            if selection.old_side && fully_mounted(selection) {
                return;
            }
            if event
                .clipboard_data()
                .is_some_and(|data| data.set_data("text/plain", &text.text).is_ok())
            {
                event.prevent_default();
                toast.ok(copy_status_toast(text.status));
            }
        }
        Ok(None) => {}
        Err(()) => {
            event.prevent_default();
            toast.info(localized_toast(SelectionCopyMessage::Pending));
            defer.call(selection);
        }
    }
}

fn copy_status_toast(status: ContextCopyStatus) -> ToastText {
    ToastText::localized(move |language| status.message(language))
}

/// Formats one argument-free selection message when its toast renders.
fn localized_toast(message: SelectionCopyMessage) -> ToastText {
    ToastText::localized(move |language| match message {
        SelectionCopyMessage::Failed => t!(language, "copy-selection-failed"),
        SelectionCopyMessage::Empty => t!(language, "copy-selection-empty"),
        SelectionCopyMessage::Pending => t!(language, "copy-selection-pending"),
    })
}

#[derive(Clone, Copy)]
enum SelectionCopyMessage {
    Failed,
    Empty,
    Pending,
}

fn selection(workspace: &ClientDiffWorkspace) -> Option<Selection> {
    let selection = web_sys::window()?.get_selection().ok()??;
    if selection.is_collapsed() {
        return None;
    }
    if let Some(selection) = whole_document(&selection, workspace) {
        return Some(selection);
    }
    let (anchor, anchor_old) = position(&selection.anchor_node()?, workspace)?;
    let (focus, focus_old) = position(&selection.focus_node()?, workspace)?;
    Some(Selection {
        start: anchor.min(focus),
        end: anchor.max(focus),
        old_side: anchor_old && focus_old,
    })
}

fn whole_document(
    selection: &web_sys::Selection,
    workspace: &ClientDiffWorkspace,
) -> Option<Selection> {
    let document = web_sys::window()?.document()?;
    let root = document.query_selector("[data-gtl-diff-document]").ok()??;
    let range = selection.get_range_at(0).ok()?;
    if range.compare_point(&root, 0).ok()? != 0
        || range
            .compare_point(&root, root.child_nodes().length())
            .ok()?
            != 0
    {
        return None;
    }
    let mut files = workspace
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.summary.row_count > 0);
    let (first, first_file) = files.next()?;
    let (last, last_file) = files.next_back().unwrap_or((first, first_file));
    Some(Selection {
        start: Position {
            file: first,
            row: 0,
        },
        end: Position {
            file: last,
            row: last_file.summary.row_count - 1,
        },
        old_side: false,
    })
}

fn position(node: &web_sys::Node, workspace: &ClientDiffWorkspace) -> Option<(Position, bool)> {
    let element = node
        .dyn_ref::<web_sys::Element>()
        .cloned()
        .or_else(|| node.parent_element())?;
    let file: usize = element
        .closest("[data-file-index]")
        .ok()??
        .get_attribute("data-file-index")?
        .parse()
        .ok()?;
    let row: usize = element
        .closest("[data-row-index]")
        .ok()??
        .get_attribute("data-row-index")?
        .parse()
        .ok()?;
    if row >= workspace.files.get(file)?.summary.row_count {
        return None;
    }
    let old = element.closest("[data-gtl-copy-line]").ok()?.is_none();
    Some((Position { file, row }, old))
}

fn fully_mounted(selection: Selection) -> bool {
    if selection.start.file != selection.end.file {
        return false;
    }
    let Some(file) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| {
            document.get_element_by_id(&format!("viewer-diff-{}", selection.start.file))
        })
    else {
        return false;
    };
    let Ok(rows) = file.query_selector_all("[data-row-index]") else {
        return false;
    };
    let count = (0..rows.length())
        .filter_map(|index| {
            rows.item(index)?
                .dyn_into::<web_sys::Element>()
                .ok()?
                .get_attribute("data-row-index")?
                .parse::<usize>()
                .ok()
        })
        .filter(|row| *row >= selection.start.row && *row <= selection.end.row)
        .count();
    count == selection.end.row - selection.start.row + 1
}

fn cached_text(
    workspace: &ClientDiffWorkspace,
    selection: Selection,
) -> Result<Option<ContextualizedCopy>, ()> {
    let mut sections = Vec::new();
    for file_index in selection.start.file..=selection.end.file {
        let file = workspace.files.get(file_index).ok_or(())?;
        let selected = cached_lines(
            &file.rows,
            selection.rows(file_index, file.summary.row_count),
            selection.old_side,
        )?;
        if let Some(text) = selected.with_context(
            &file.summary.path.to_string_lossy(),
            super::super::file::copy_comment_leader(&file.summary.path),
        ) {
            sections.push(text);
        }
    }
    Ok(join_sections(sections))
}

fn cached_lines(
    rows: &crate::entities::diffs::ClientDiffRows,
    range: std::ops::Range<usize>,
    old_side: bool,
) -> Result<SelectedDiffLines, ()> {
    let mut selected = SelectedDiffLines::default();
    for row in range {
        if let Some((number, text)) = cached::line(rows, row, old_side)? {
            selected.push(text.to_owned(), Some(number));
        }
    }
    Ok(selected)
}

async fn read_text(
    identity: ViewerViewIdentity,
    workspace: ReadStore<ClientDiffWorkspace>,
    selection: Selection,
) -> Result<Option<ContextualizedCopy>, ViewerClientError> {
    let mut sections = Vec::new();
    for file_index in selection.start.file..=selection.end.file {
        let summary = workspace
            .peek()
            .files
            .get(file_index)
            .ok_or(ViewerClientError::Failed(Failure::Changed))?
            .summary
            .clone();
        let rows = selection.rows(file_index, summary.row_count);
        let selected = read_lines(identity, summary.id.clone(), rows, selection.old_side).await?;
        if let Some(text) = selected.with_context(
            &summary.path.to_string_lossy(),
            super::super::file::copy_comment_leader(&summary.path),
        ) {
            sections.push(text);
        }
    }
    Ok(join_sections(sections))
}

async fn read_lines(
    identity: ViewerViewIdentity,
    file: gtl_wire::viewer::ViewerDiffFileId,
    rows: std::ops::Range<usize>,
    old_side: bool,
) -> Result<SelectedDiffLines, ViewerClientError> {
    let mut selected = SelectedDiffLines::default();
    for start in (rows.start..rows.end).step_by(64) {
        let count = (rows.end - start).min(64);
        let request = ReadViewerDiffText {
            identity,
            file: file.clone(),
            row_range: ViewerRowRange::try_new(
                u32::try_from(start).map_err(|_| ViewerClientError::InvalidMessage)?,
                u32::try_from(count).map_err(|_| ViewerClientError::InvalidMessage)?,
            )
            .map_err(|_| ViewerClientError::InvalidMessage)?,
            old_side,
        };
        for ViewerDiffTextLine { line_number, text } in
            viewer_server::read_diff_text(request).await?
        {
            selected.push(text, Some(line_number));
        }
    }
    Ok(selected)
}

fn join_sections(mut sections: Vec<ContextualizedCopy>) -> Option<ContextualizedCopy> {
    if sections.len() == 1 {
        return sections.pop();
    }
    if sections.is_empty() {
        return None;
    }
    Some(ContextualizedCopy {
        status: ContextCopyStatus::Files(sections.len()),
        text: sections
            .into_iter()
            .map(|section| section.text)
            .collect::<Vec<_>>()
            .join("\n\n"),
    })
}
