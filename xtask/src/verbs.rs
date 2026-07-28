//! Automation verbs — one module per verb; each owns its flags and workflow.

pub(crate) mod ast_grep;
pub(crate) mod bench;
pub(crate) mod bootstrap;
pub(crate) mod build;
pub(crate) mod check;
pub(crate) mod check_structure;
pub(crate) mod cov;
pub(crate) mod desktop_e2e;
pub(crate) mod drift;
pub(crate) mod fix;
pub(crate) mod format;
pub(crate) mod frontend;
pub(crate) mod icon;
pub(crate) mod install;
pub(crate) mod lint;
pub(crate) mod pre_commit;
pub(crate) mod presentation;
pub(crate) mod ship;
pub(crate) mod status_notifier;
pub(crate) mod test;
