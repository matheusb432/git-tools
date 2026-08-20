#[cfg(feature = "desktop")]
mod app;
#[cfg(feature = "artifact")]
mod artifact;
#[cfg(any(feature = "artifact", feature = "desktop"))]
mod entities;
#[cfg(any(feature = "artifact", feature = "desktop"))]
mod shared;
#[cfg(test)]
mod test_support;
#[cfg(any(feature = "artifact", feature = "desktop"))]
mod views;

#[cfg(feature = "artifact")]
pub use artifact::{
    StaticArtifactFileSource, StaticArtifactView, StaticArtifactViewError,
    render_static_artifact_body, static_artifact_enhancement_script,
};
#[cfg(any(feature = "artifact", feature = "desktop"))]
pub use views::diffs::diff_workspace::commits_panel::{CommitsPanel, CommitsPanelProps};

#[cfg(feature = "desktop")]
pub fn launch_desktop() {
    dioxus::launch(app::App);
}
