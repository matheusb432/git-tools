use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use gtl_wire::viewer::ViewerFileStatus;

use crate::shared::i18n::{t, use_language};

#[component]
pub(crate) fn DiffFileStatus(status: ViewerFileStatus) -> Element {
    let label = status_label(status, use_language());
    let color = file_status_text_class(status);

    rsx! {
        span {
            class: "flex-none text-xs leading-none font-semibold {color}",
            title: label.clone(),
            aria_label: label,
            "{status_code(status)}"
        }
    }
}

pub(crate) const fn file_status_text_class(status: ViewerFileStatus) -> &'static str {
    match status {
        ViewerFileStatus::Added => "text-add-ink",
        ViewerFileStatus::Deleted => "text-del-ink",
        ViewerFileStatus::Renamed => "text-acc",
        ViewerFileStatus::Modified => "text-ink-2",
    }
}

const fn status_code(status: ViewerFileStatus) -> &'static str {
    match status {
        ViewerFileStatus::Added => "A",
        ViewerFileStatus::Deleted => "D",
        ViewerFileStatus::Renamed => "R",
        ViewerFileStatus::Modified => "M",
    }
}

fn status_label(status: ViewerFileStatus, language: ViewerLanguage) -> String {
    match status {
        ViewerFileStatus::Added => t!(language, "file-status-added"),
        ViewerFileStatus::Deleted => t!(language, "file-status-deleted"),
        ViewerFileStatus::Renamed => t!(language, "file-status-renamed"),
        ViewerFileStatus::Modified => t!(language, "file-status-modified"),
    }
}
