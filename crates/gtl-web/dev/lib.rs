mod preview;
mod stories;

pub(super) fn launch() -> Result<(), dx_book::StoryRegistryError> {
    dx_book::launch(preview::App)
}
