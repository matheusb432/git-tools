#[cfg(feature = "desktop")]
mod app;
#[cfg(feature = "artifact")]
mod artifact;
#[cfg(any(feature = "artifact", feature = "desktop"))]
mod entities;
#[cfg(any(feature = "artifact", feature = "desktop"))]
mod shared;
#[cfg(any(feature = "artifact", feature = "desktop"))]
mod views;

#[cfg(feature = "desktop")]
pub fn launch_desktop() {
    dioxus::launch(app::App);
}

#[cfg(feature = "artifact")]
pub fn launch_artifact() {
    dioxus::launch(artifact::ArtifactApp);
}
