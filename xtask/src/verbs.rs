//! Automation verbs — one module per verb; each owns its flags and workflow.

pub(crate) mod bench;
pub(crate) mod build;
pub(crate) mod check_structure;
pub(crate) mod desktop_e2e;
pub(crate) mod desktop_release;
pub(crate) mod dioxus_web;
pub(crate) mod drift;
pub(crate) mod format;
pub(crate) mod icon;
pub(crate) mod install;
pub(crate) mod pre_commit;
pub(crate) mod setup;
pub(crate) mod ship;
pub(crate) mod status_notifier;
pub(crate) mod test;
