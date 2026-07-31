use application::diffs::FileStatus;

#[derive(Clone, Copy)]
pub(super) struct FileStatusPresentation {
    pub(super) css_class: &'static str,
    pub(super) code: &'static str,
    pub(super) label: &'static str,
    pub(super) badge_classes: &'static str,
}

const ADDED: FileStatusPresentation = FileStatusPresentation {
    css_class: "status-added",
    code: "A",
    label: "Added file",
    badge_classes: "border-add-line bg-add-bg text-add",
};
const DELETED: FileStatusPresentation = FileStatusPresentation {
    css_class: "status-deleted",
    code: "D",
    label: "Deleted file",
    badge_classes: "border-del-line bg-del-bg text-del",
};
const RENAMED: FileStatusPresentation = FileStatusPresentation {
    css_class: "status-renamed",
    code: "R",
    label: "Renamed file",
    badge_classes: "border-acc-line bg-acc-soft text-acc",
};
const MODIFIED: FileStatusPresentation = FileStatusPresentation {
    css_class: "status-modified",
    code: "M",
    label: "Modified file",
    badge_classes: "border-line-2 bg-sunk text-ink-3",
};

pub(super) const fn file_status_presentation(status: FileStatus) -> FileStatusPresentation {
    match status {
        FileStatus::Added => ADDED,
        FileStatus::Deleted => DELETED,
        FileStatus::Renamed => RENAMED,
        FileStatus::Modified => MODIFIED,
    }
}
