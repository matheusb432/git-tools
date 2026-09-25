#[cfg(feature = "desktop")]
mod app;
#[cfg(feature = "artifact")]
mod artifact;
#[cfg(feature = "component-preview")]
#[path = "../dev/lib.rs"]
mod component_preview;
#[cfg(any(feature = "artifact", feature = "desktop"))]
mod entities;
#[cfg(any(feature = "artifact", feature = "interactive-ui"))]
mod shared;
#[cfg(all(test, any(feature = "artifact", feature = "desktop")))]
mod test_support;
#[cfg(any(feature = "artifact", feature = "desktop"))]
mod views;

#[cfg(feature = "artifact")]
pub use artifact::{
    StaticArtifactFileRows, StaticArtifactView, StaticArtifactViewError,
    render_static_artifact_body, static_artifact_document_title,
    static_artifact_enhancement_script,
};
#[cfg(any(feature = "artifact", feature = "desktop"))]
pub use views::diffs::diff_workspace::commits_panel::{CommitsPanel, CommitsPanelProps};

#[cfg(feature = "desktop")]
pub fn launch_desktop() {
    dioxus::launch(app::App);
}

#[cfg(feature = "component-preview")]
pub fn launch_component_preview() -> Result<(), dx_story::RegistryError> {
    component_preview::launch()
}
