use dioxus::prelude::*;
use gtl_wire::viewer::ViewerFileStatus;

#[component]
pub(crate) fn DiffFileStatus(status: ViewerFileStatus) -> Element {
    let label = status_label(status);
    let color = file_status_text_class(status);

    rsx! {
        span {
            class: "flex-none text-xs leading-none font-semibold {color}",
            title: label,
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

const fn status_label(status: ViewerFileStatus) -> &'static str {
    match status {
        ViewerFileStatus::Added => "Added file",
        ViewerFileStatus::Deleted => "Deleted file",
        ViewerFileStatus::Renamed => "Renamed file",
        ViewerFileStatus::Modified => "Modified file",
    }
}
