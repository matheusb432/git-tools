use gtl_models::{diffs::DiffExclusions, viewer::RenderOptions};

/// One effective snapshot of the user configuration used by application operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    theme: Option<String>,
    viewer_render_options: RenderOptions,
    push_confirmation_required: bool,
    diff_exclusions: DiffExclusions,
}

impl AppSettings {
    /// Creates an effective settings snapshot from application and models values.
    pub fn new(
        theme: Option<String>,
        push_confirmation_required: bool,
        diff_exclusions: DiffExclusions,
    ) -> Self {
        Self {
            theme,
            viewer_render_options: RenderOptions::DEFAULT,
            push_confirmation_required,
            diff_exclusions,
        }
    }

    /// Returns this snapshot with the selected viewer layout and density.
    #[must_use]
    pub const fn with_viewer_render_options(
        mut self,
        viewer_render_options: RenderOptions,
    ) -> Self {
        self.viewer_render_options = viewer_render_options;
        self
    }

    /// Returns the effective viewer layout and density.
    pub const fn viewer_render_options(&self) -> RenderOptions {
        self.viewer_render_options
    }

    /// The raw diff-preview theme, when the renderer should force one.
    pub fn theme(&self) -> Option<&str> {
        self.theme.as_deref()
    }

    /// Whether a plain current-repository push requires confirmation.
    pub const fn push_confirmation_required(&self) -> bool {
        self.push_confirmation_required
    }

    /// The complete project and default diff-exclusion map.
    pub const fn diff_exclusions(&self) -> &DiffExclusions {
        &self.diff_exclusions
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self::new(None, true, DiffExclusions::default())
    }
}

pub trait UserSettingsStore: Clone + Send + Sync + 'static {
    /// Loads the current snapshot, degrading adapter failures to safe defaults.
    fn load(&self) -> AppSettings;
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::RenderOptions;

    use super::AppSettings;

    #[test]
    fn app_settings_default_preserves_safe_behavior() {
        let settings = AppSettings::default();

        assert_eq!(settings.theme(), None);
        assert_eq!(settings.viewer_render_options(), RenderOptions::DEFAULT);
        assert!(settings.push_confirmation_required());
        assert!(settings.diff_exclusions().is_empty());
    }
}
