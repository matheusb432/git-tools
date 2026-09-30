use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::ViewerFileStatus;

use super::changes_since::ChangesSinceChoice;
use crate::app::application_layout::{ViewerContext, ViewerShellLoad};

/// A rename counts as a modification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileChangeKind {
    Added,
    Removed,
    Modified,
}

impl FileChangeKind {
    pub(crate) const ALL: [Self; 3] = [Self::Added, Self::Removed, Self::Modified];

    pub(crate) const fn of(status: ViewerFileStatus) -> Self {
        match status {
            ViewerFileStatus::Added => Self::Added,
            ViewerFileStatus::Deleted => Self::Removed,
            ViewerFileStatus::Modified | ViewerFileStatus::Renamed => Self::Modified,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Modified => "modified",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShownFileChanges {
    added: bool,
    removed: bool,
    modified: bool,
}

impl Default for ShownFileChanges {
    fn default() -> Self {
        Self {
            added: true,
            removed: true,
            modified: true,
        }
    }
}

impl ShownFileChanges {
    pub(crate) const fn shows(self, kind: FileChangeKind) -> bool {
        match kind {
            FileChangeKind::Added => self.added,
            FileChangeKind::Removed => self.removed,
            FileChangeKind::Modified => self.modified,
        }
    }

    pub(crate) const fn shows_status(self, status: ViewerFileStatus) -> bool {
        self.shows(FileChangeKind::of(status))
    }

    #[must_use]
    const fn toggled(self, kind: FileChangeKind) -> Self {
        match kind {
            FileChangeKind::Added => Self {
                added: !self.added,
                ..self
            },
            FileChangeKind::Removed => Self {
                removed: !self.removed,
                ..self
            },
            FileChangeKind::Modified => Self {
                modified: !self.modified,
                ..self
            },
        }
    }

    pub(crate) const fn shows_all(self) -> bool {
        self.added && self.removed && self.modified
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FileFilterForm {
    pub(crate) shown: ShownFileChanges,
    pub(crate) text: String,
    /// The server applies the cutoff; the form remembers how it was chosen.
    pub(crate) changes_since: ChangesSinceChoice,
}

impl FileFilterForm {
    pub(crate) fn is_active(&self) -> bool {
        !self.shown.shows_all() || !self.text.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileFilterEdit {
    Text(String),
    Toggle(FileChangeKind),
    ChangesSince(ChangesSinceChoice),
    Clear,
}

impl FileFilterEdit {
    pub(crate) fn apply(self, form: &mut FileFilterForm) {
        match self {
            Self::Text(text) => form.text = text,
            Self::Toggle(kind) => form.shown = form.shown.toggled(kind),
            Self::ChangesSince(choice) => form.changes_since = choice,
            Self::Clear => *form = FileFilterForm::default(),
        }
    }
}

/// Owns every open tab's file filter form until the tab closes.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct FileFilterForms {
    forms: Signal<HashMap<ViewerTabId, FileFilterForm>>,
}

pub(crate) fn use_file_filter_forms_provider() {
    let mut forms = use_signal(HashMap::<ViewerTabId, FileFilterForm>::new);
    use_context_provider(|| FileFilterForms { forms });
    let viewer = use_context::<ViewerContext>();
    use_effect(move || {
        let shell = viewer.shell();
        let shell = shell.read();
        let ViewerShellLoad::Ready(shell) = &*shell else {
            return;
        };
        let open = shell.tabs.iter().map(|tab| tab.id).collect::<HashSet<_>>();
        if forms.peek().keys().any(|tab| !open.contains(tab)) {
            forms.write().retain(|tab, _| open.contains(tab));
        }
    });
}

impl FileFilterForms {
    pub(crate) fn form(self, tab: ViewerTabId) -> FileFilterForm {
        self.forms.read().get(&tab).cloned().unwrap_or_default()
    }

    pub(crate) fn edit(mut self, tab: ViewerTabId, edit: FileFilterEdit) {
        let current = self.forms.peek().get(&tab).cloned().unwrap_or_default();
        let mut form = current.clone();
        edit.apply(&mut form);
        if form == current {
            return;
        }
        let mut forms = self.forms.write();
        if form == FileFilterForm::default() {
            forms.remove(&tab);
        } else {
            forms.insert(tab, form);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renamed_files_count_as_modified() {
        let shown = ShownFileChanges::default().toggled(FileChangeKind::Modified);

        assert!(!shown.shows_status(ViewerFileStatus::Renamed));
        assert!(!shown.shows_status(ViewerFileStatus::Modified));
        assert!(shown.shows_status(ViewerFileStatus::Added));
        assert!(shown.shows_status(ViewerFileStatus::Deleted));
    }

    #[test]
    fn clearing_restores_every_change_kind_and_the_empty_query() {
        let mut form = FileFilterForm::default();
        FileFilterEdit::Toggle(FileChangeKind::Removed).apply(&mut form);
        FileFilterEdit::Text("create".to_owned()).apply(&mut form);
        assert!(form.is_active());

        FileFilterEdit::Clear.apply(&mut form);

        assert_eq!(form, FileFilterForm::default());
        assert!(!form.is_active());
    }
}
