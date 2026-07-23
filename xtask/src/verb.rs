//! Verb names shared by the clap surface and the `RESULT` output contract.

/// A verb name constant: the clap subcommand spelling and the `RESULT scope=…` value are the same
/// string, so a scope can never be typoed at a call site.
#[derive(Clone, Copy)]
pub(crate) struct Verb(&'static str);

impl Verb {
    pub(crate) const BOOTSTRAP: Self = Self("bootstrap");
    pub(crate) const BUILD: Self = Self("build");
    pub(crate) const CHECK: Self = Self("check");
    pub(crate) const CHECK_DEPS: Self = Self("check-deps");
    pub(crate) const CHECK_STRUCTURE: Self = Self("check-structure");
    pub(crate) const DESKTOP_BENCH: Self = Self("desktop-bench");
    pub(crate) const DESKTOP_E2E: Self = Self("desktop-e2e");
    pub(crate) const DRIFT_CHECK: Self = Self("drift-check");
    pub(crate) const FIX: Self = Self("fix");
    pub(crate) const FORMAT: Self = Self("fmt");
    pub(crate) const FORMAT_CHECK: Self = Self("fmt-check");
    pub(crate) const FRONTEND_BENCH: Self = Self("frontend-bench");
    pub(crate) const FRONTEND_TEST: Self = Self("frontend-test");
    pub(crate) const LINT: Self = Self("lint");
    pub(crate) const PRE_COMMIT: Self = Self("pre-commit");
    pub(crate) const GEN_ICON: Self = Self("gen-icon");
    pub(crate) const INSTALL: Self = Self("install");
    pub(crate) const SHIP: Self = Self("ship");
    pub(crate) const TEST: Self = Self("test");
    pub(crate) const UNINSTALL: Self = Self("uninstall");

    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for Verb {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
