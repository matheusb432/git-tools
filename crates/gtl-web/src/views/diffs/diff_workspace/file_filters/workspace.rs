use dioxus::{core::spawn_forever, prelude::*};
use gtl_models::{
    diffs::ExtensionFilter, settings::DiffFilesSort, timestamps::MachineTimestamp,
    viewer::ViewerTabId,
};
use gtl_wire::viewer::{SetViewerChangesSince, ViewerActiveView, ViewerDiffFileId};

use crate::{
    app::application_layout::ViewerContext,
    entities::diffs::viewer_server,
    shared::{browser, ui::use_toast},
    views::diffs::{
        changes_since::{ChangesSinceEdit, CutoffChange},
        displayed_files::{displayed_view, searchable_files},
        file_filter_changes::FileFilterController,
        file_filter_form::{FileFilterEdit, FileFilterForm, FileFilterForms},
        text_filter::{TextFilter, use_text_filter},
    },
};

#[derive(Clone, Copy)]
pub(crate) struct WorkspaceFileFilters {
    pub(crate) review_progress: Memo<(usize, usize)>,
    pub(crate) form: Memo<FileFilterForm>,
    pub(crate) edit: Callback<FileFilterEdit>,
    pub(crate) text: TextFilter,
    pub(crate) changes_since: ChangesSinceFilter,
    pub(crate) hidden_count: Memo<usize>,
    pub(crate) active: Memo<bool>,
    pub(crate) clear: Callback<()>,
}

impl WorkspaceFileFilters {
    pub(crate) fn review_complete(self) -> bool {
        let (reviewed, total) = (self.review_progress)();
        self.form.read().unreviewed && total > 0 && reviewed == total
    }
}

/// The server-applied cutoff of the active tab.
#[derive(Clone, Copy)]
pub(crate) struct ChangesSinceFilter {
    pub(crate) applied: Memo<Option<MachineTimestamp>>,
    pub(crate) available: Memo<bool>,
    pub(crate) edit: Callback<ChangesSinceEdit>,
    apply: Callback<Option<MachineTimestamp>>,
}

/// Provides the active tab's filters and returns them with the view they display.
pub(in crate::views::diffs::diff_workspace) fn use_workspace_file_filters(
    source: ReadSignal<ViewerActiveView>,
    files_sort: Memo<DiffFilesSort>,
) -> (Memo<ViewerActiveView>, WorkspaceFileFilters) {
    let forms = use_context::<FileFilterForms>();
    let tab_id = use_memo(move || source.read().identity.tab_id);
    let form = use_memo(move || forms.form(tab_id()));
    let edit = use_callback(move |edit: FileFilterEdit| forms.edit(tab_id(), edit));
    let review_progress = use_memo(move || {
        let source = source.read();
        (
            source
                .files
                .iter()
                .filter(|file| file.review.as_ref().is_some_and(|review| review.reviewed))
                .count(),
            source.files.len(),
        )
    });
    let searchable = use_memo(move || {
        let form = form.read();
        searchable_files(&source.read(), files_sort(), form.shown, form.unreviewed)
    });
    let searched_files = use_memo(move || {
        searchable
            .read()
            .iter()
            .map(|file| file.id.clone())
            .collect::<Vec<ViewerDiffFileId>>()
    });
    let query = use_memo(move || form.read().text.clone());
    let set_query = use_callback(move |text: String| edit.call(FileFilterEdit::Text(text)));
    let text = use_text_filter(source, query, searched_files, set_query);
    let displayed = use_memo(move || {
        displayed_view(
            &source.read(),
            &searchable.read(),
            text.matched_files.read().as_ref(),
        )
    });
    let hidden_count = use_memo(move || {
        let source = source.read();
        let extension_hidden = source
            .extension_filter
            .as_ref()
            .map_or(0, |applied| applied.hidden_paths.len());
        extension_hidden
            + source
                .files
                .len()
                .saturating_sub(displayed.read().files.len())
    });
    let extension_active = use_extension_filter_active(source, tab_id);
    let changes_since = use_changes_since_filter(source, tab_id, edit);
    let active = use_memo(move || {
        form.read().is_active() || extension_active() || changes_since.applied.read().is_some()
    });
    let extensions = use_context::<FileFilterController>();
    let clear = use_callback(move |()| {
        edit.call(FileFilterEdit::Clear);
        if *extension_active.peek() {
            extensions.submit(*tab_id.peek(), ExtensionFilter::default());
        }
        if changes_since.applied.peek().is_some() {
            changes_since.apply.call(None);
        }
    });
    let filters = use_context_provider(|| WorkspaceFileFilters {
        review_progress,
        form,
        edit,
        text,
        changes_since,
        hidden_count,
        active,
        clear,
    });
    (displayed, filters)
}

pub(crate) fn use_workspace_file_filters_context() -> WorkspaceFileFilters {
    use_context::<WorkspaceFileFilters>()
}

fn use_extension_filter_active(
    source: ReadSignal<ViewerActiveView>,
    tab_id: Memo<ViewerTabId>,
) -> Memo<bool> {
    let extensions = use_context::<FileFilterController>();
    use_memo(move || {
        extensions.displayed(tab_id(), u64::MAX).map_or_else(
            || {
                source
                    .read()
                    .extension_filter
                    .as_ref()
                    .is_some_and(|applied| applied.filter.is_active())
            },
            |pending| pending.is_active(),
        )
    })
}

fn use_changes_since_filter(
    source: ReadSignal<ViewerActiveView>,
    tab_id: Memo<ViewerTabId>,
    edit_form: Callback<FileFilterEdit>,
) -> ChangesSinceFilter {
    let viewer = use_context::<ViewerContext>();
    let toast = use_toast();
    let applied = use_memo(move || source.read().changes_since.clone());
    let available = use_memo(move || !source.read().modified_files);
    let apply = use_callback(move |changes_since: Option<MachineTimestamp>| {
        let request = SetViewerChangesSince {
            tab_id: *tab_id.peek(),
            changes_since,
        };
        spawn_forever(async move {
            match viewer_server::set_changes_since(request).await {
                Ok(shell) => viewer.replace_shell(shell),
                Err(error) => toast.client_error(&error),
            }
        });
    });
    let edit = use_callback(move |change: ChangesSinceEdit| {
        let Some((choice, cutoff)) =
            change.resolve(browser::current_timestamp(), browser::local_offset_at)
        else {
            return;
        };
        edit_form.call(FileFilterEdit::ChangesSince(choice));
        let requested = match cutoff {
            CutoffChange::Keep => return,
            CutoffChange::Clear => None,
            CutoffChange::Apply(cutoff) => Some(cutoff),
        };
        if *applied.peek() != requested {
            apply.call(requested);
        }
    });
    ChangesSinceFilter {
        applied,
        available,
        edit,
        apply,
    }
}
