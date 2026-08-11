#[cfg(feature = "desktop")]
mod app;
#[cfg(feature = "desktop")]
mod entities;
#[cfg(feature = "desktop")]
mod shared;
#[cfg(feature = "desktop")]
mod views;

#[cfg(feature = "desktop")]
pub fn launch_desktop() {
    dioxus::launch(app::App);
}
