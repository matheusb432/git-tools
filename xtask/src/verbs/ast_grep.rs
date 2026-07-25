//! ast-grep check gate.

use std::path::Path;

use crate::task::Step;

const CONFIG_FILE: &str = "sgconfig.yml";

pub(super) fn check_step() -> Option<Step> {
    Path::new(CONFIG_FILE)
        .is_file()
        .then(|| Step::new("check-ast-grep-rules", "ast-grep", ["scan"]))
}
