mod preview;
mod stories;

pub(super) fn launch() {
    dioxus::launch(preview::App);
}
