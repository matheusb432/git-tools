mod preview;
mod stories;

pub(super) fn launch() -> Result<(), dx_story::RegistryError> {
    dx_story::launch(preview::App)
}
