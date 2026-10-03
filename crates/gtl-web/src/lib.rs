mod app;
#[cfg(feature = "benchmark-support")]
pub mod benchmark;
#[cfg(feature = "component-preview")]
#[path = "../dev/lib.rs"]
mod component_preview;
mod entities;
mod shared;
#[cfg(test)]
mod test_support;
mod views;

pub use views::diffs::diff_workspace::commits_panel::{CommitsPanel, CommitsPanelProps};

pub fn launch_desktop() {
    dioxus::launch(app::App);
}

#[cfg(feature = "component-preview")]
pub fn launch_component_preview() -> Result<(), dx_story::RegistryError> {
    component_preview::launch()
}
