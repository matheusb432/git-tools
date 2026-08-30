mod preview;
mod showcases;

pub(super) fn launch() -> Result<(), dx_preview::RegistryError> {
    dx_preview::launch(preview::App)
}
