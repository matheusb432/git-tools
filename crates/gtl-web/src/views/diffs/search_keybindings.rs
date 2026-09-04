pub(super) const SEARCH_FILES_KEY_BINDING: [&str; 2] = ["Ctrl", "P"];
#[cfg(feature = "component-preview")]
pub(super) const SEARCH_TEXT_IN_FILE_KEY_BINDING: [&str; 2] = ["Ctrl", "F"];
#[cfg(any(feature = "component-preview", feature = "desktop"))]
pub(super) const SEARCH_TEXT_IN_ALL_FILES_KEY_BINDING: [&str; 3] = ["Ctrl", "Shift", "F"];
