use dioxus::prelude::*;
use dx_story::{stories, story};
use gtl_models::diffs::{ExtensionFilter, ExtensionFilterMode, FileExtensions};

use crate::views::diffs::{
    changes_since::{ChangesSinceEdit, ChangesSinceSelection},
    diff_workspace::file_filters::FileFiltersMenu,
    file_filter_form::{FileFilterEdit, FileFilterForm},
};

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        Demo {}
    }
}

/// Text, change kind, changed-since, and extension filters with one clear action.
#[story]
fn interactive() -> Element {
    rsx! {
        div { class: "min-h-[44rem]", Demo {} }
    }
}

#[component]
fn Demo() -> Element {
    let mut form = use_signal(|| FileFilterForm {
        text: "create".to_owned(),
        ..FileFilterForm::default()
    });
    let mut extension_filter = use_signal(|| {
        ExtensionFilter::new(
            ExtensionFilterMode::Hide,
            FileExtensions::new(["json", "lock"]),
        )
    });
    let mut changes_since = use_signal(|| ChangesSinceSelection::AnyTime);
    let active = form.read().is_active()
        || extension_filter.read().is_active()
        || changes_since() != ChangesSinceSelection::AnyTime;
    let mut edit = move |edit: FileFilterEdit| edit.apply(&mut form.write());
    rsx! {
        div { class: "flex w-64 items-center justify-between rounded-panel border border-line bg-surface p-4",
            span { class: "font-semibold text-ink", "Files" }
            FileFiltersMenu {
                id: "story-diff-file-filters",
                text: form.read().text.clone(),
                text_status: "8 matches",
                ontext: move |text| edit(FileFilterEdit::Text(text)),
                shown: form.read().shown,
                ontoggle: move |kind| edit(FileFilterEdit::Toggle(kind)),
                changes_since: changes_since(),
                onchangessince: move |change| {
                    changes_since
                        .set(
                            match change {
                                ChangesSinceEdit::AnyTime => ChangesSinceSelection::AnyTime,
                                ChangesSinceEdit::Preset(preset) => {
                                    ChangesSinceSelection::Preset(preset)
                                }
                                ChangesSinceEdit::Custom | ChangesSinceEdit::CustomValue(_) => {
                                    ChangesSinceSelection::Custom
                                }
                            },
                        );
                },
                extension_filter: extension_filter(),
                extensions: ["css", "html", "json", "lock", "md", "rs", "toml", "ts"]
                    .map(str::to_owned)
                    .to_vec(),
                onextension: move |next| extension_filter.set(next),
                hidden_count: 3,
                active,
                onclear: move |()| {
                    edit(FileFilterEdit::Clear);
                    extension_filter.set(ExtensionFilter::default());
                    changes_since.set(ChangesSinceSelection::AnyTime);
                },
            }
        }
    }
}

#[stories(id = "file-filters", name = "File filters", thumbnail = thumbnail)]
const FILE_FILTERS_STORIES: () = &[interactive];
