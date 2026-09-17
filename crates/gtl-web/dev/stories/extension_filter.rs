use dioxus::prelude::*;
use dx_story::{stories, story};
use gtl_models::diffs::ExcludedExtensions;

use crate::{
    shared::ui::ExtensionExclusionsAction,
    views::diffs::diff_workspace::extension_filters::ExtensionFilter,
};

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        Demo {}
    }
}

/// Excluded chips with a searchable multiselect and immediate selection feedback.
#[story]
fn interactive() -> Element {
    rsx! {
        div { class: "min-h-[34rem]", Demo {} }
    }
}

#[component]
fn Demo() -> Element {
    let mut excluded = use_signal(|| ExcludedExtensions::new(["json", "lock"]));
    rsx! {
        div { class: "flex justify-end rounded-panel border border-line bg-surface p-4",
            ExtensionFilter {
                excluded: excluded(),
                available: ["css", "html", "json", "lock", "md", "rs", "toml", "ts"]
                    .map(str::to_owned)
                    .to_vec(),
                onchange: move |action: ExtensionExclusionsAction| {
                    let next = action.apply(&excluded.peek());
                    excluded.set(next);
                },
                onrestore: move |()| excluded.set(ExcludedExtensions::new(["lock"])),
            }
        }
    }
}

#[stories(id = "extension-filter", name = "Excluded extensions", thumbnail = thumbnail)]
const EXTENSION_FILTER_STORIES: () = &[interactive];
