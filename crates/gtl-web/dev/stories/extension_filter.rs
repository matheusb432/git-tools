use dioxus::prelude::*;
use dx_story::{stories, story};
use gtl_models::diffs::{ExtensionFilter, ExtensionFilterMode, FileExtensions};

use crate::views::diffs::diff_workspace::extension_filters::ExtensionFilterMenu;

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        Demo {}
    }
}

/// Show-only and hide modes over removable chips and a searchable multiselect.
#[story]
fn interactive() -> Element {
    rsx! {
        div { class: "min-h-[34rem]", Demo {} }
    }
}

#[component]
fn Demo() -> Element {
    let mut filter = use_signal(|| {
        ExtensionFilter::new(
            ExtensionFilterMode::Hide,
            FileExtensions::new(["json", "lock"]),
        )
    });
    rsx! {
        div { class: "flex w-64 items-center justify-between rounded-panel border border-line bg-surface p-4",
            span { class: "font-semibold text-ink", "Files" }
            ExtensionFilterMenu {
                id: "story-diff-extension-filters",
                filter: filter(),
                available: ["css", "html", "json", "lock", "md", "rs", "toml", "ts"]
                    .map(str::to_owned)
                    .to_vec(),
                hidden_count: 3,
                onchange: move |next| filter.set(next),
            }
        }
    }
}

#[stories(id = "extension-filter", name = "Extension filter", thumbnail = thumbnail)]
const EXTENSION_FILTER_STORIES: () = &[interactive];
