#[cfg(feature = "desktop")]
mod app;
#[cfg(feature = "artifact")]
mod artifact;
#[cfg(feature = "artifact")]
mod artifact_asset;
#[cfg(any(feature = "artifact", feature = "desktop"))]
mod entities;
#[cfg(any(feature = "artifact", feature = "desktop"))]
mod shared;
#[cfg(test)]
mod test_support;
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
