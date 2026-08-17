use std::path::Path;

use anyhow::Result;

use crate::task::Step;

mod dioxus;

pub(crate) fn check_dioxus() -> Result<()> {
    dioxus::check()
}

pub(crate) fn dioxus_check_step(directory: &Path) -> Step {
    dioxus::check_step(directory)
}
