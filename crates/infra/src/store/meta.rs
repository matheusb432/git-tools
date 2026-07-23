//! Per-artifact metadata (sidecar) and the diff kind that keys range lookups.
pub use domain::diffs::DiffKind;
use domain::viewer::RenderOptions;
use serde::{Deserialize, Serialize};

fn layout_default() -> String {
    RenderOptions::DEFAULT.layout().to_string()
}

fn density_default() -> String {
    RenderOptions::DEFAULT.density().to_string()
}

/// Bumped when the renderer's HTML output changes materially so range reuse never serves an
/// artifact rendered by an older renderer.
pub const RENDERER_VERSION: u32 = 2;

/// Metadata stored alongside each artifact as `<hash>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidecar {
    pub repo_id: String,
    pub repo_name: String,
    pub repo_root: String,
    pub kind: DiffKind,
    pub base_sha: String,
    pub head_sha: String,
    pub range_label: String,
    pub head_committed_at: String,
    pub generated_at: String,
    pub title: String,
    pub byte_size: u64,
    #[serde(default = "layout_default")]
    pub layout: String,
    #[serde(default = "density_default")]
    pub density: String,
    /// Configured renderer theme used for this artifact. `None` means the
    /// renderer selected its default theme.
    #[serde(default)]
    pub theme: Option<String>,
    /// Distinguishes current default-theme artifacts from legacy sidecars that
    /// predate theme metadata and must not satisfy range reuse.
    #[serde(default)]
    pub theme_recorded: bool,
    /// Renderer version that produced this artifact, compared against
    /// [`RENDERER_VERSION`] to gate range reuse. Missing (pre-feature)
    /// sidecars default to `0`. Content reuse in `place` refreshes a stale
    /// stored version to the current one.
    #[serde(default)]
    pub renderer_version: u32,
    /// Extension set in force at render time (normalized, sorted; empty =
    /// unfiltered). Defaults keep pre-exclusion sidecars readable, and their
    /// empty set correctly means "rendered without exclusions".
    #[serde(default)]
    pub excluded_extensions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use domain::viewer::{DiffDensity, DiffLayout, RenderOptions};

    use super::*;

    #[test]
    fn sidecar_round_trips_through_json() {
        let sc = Sidecar {
            repo_id: "deadbeef00000000".into(),
            repo_name: "git-tools".into(),
            repo_root: "/home/u/tools/git-tools".into(),
            kind: DiffKind::TwoDot,
            base_sha: "aaaa".into(),
            head_sha: "bbbb".into(),
            range_label: "origin/main..HEAD".into(),
            head_committed_at: "2026-06-22T10:00:00Z".into(),
            generated_at: "2026-06-22T10:01:00Z".into(),
            title: "diff".into(),
            byte_size: 1234,
            layout: DiffLayout::Split.to_string(),
            density: DiffDensity::Full.to_string(),
            theme: Some("dark".into()),
            theme_recorded: true,
            renderer_version: RENDERER_VERSION,
            excluded_extensions: vec!["md".into()],
        };
        let json = serde_json::to_string(&sc).unwrap();
        assert_eq!(serde_json::from_str::<Sidecar>(&json).unwrap(), sc);
    }

    #[test]
    fn sidecar_without_theme_metadata_is_marked_legacy() {
        let json = r#"{
            "repo_id":"deadbeef00000000",
            "repo_name":"git-tools",
            "repo_root":"/repo",
            "kind":"two_dot",
            "base_sha":"aaaa",
            "head_sha":"bbbb",
            "range_label":"a..b",
            "head_committed_at":"t",
            "generated_at":"t",
            "title":"diff",
            "byte_size":1
        }"#;

        let sidecar = serde_json::from_str::<Sidecar>(json).unwrap();

        assert_eq!(sidecar.theme, None);
        assert!(!sidecar.theme_recorded);
    }

    #[test]
    fn sidecar_without_renderer_version_defaults_to_zero() {
        let json = r#"{
            "repo_id":"deadbeef00000000",
            "repo_name":"git-tools",
            "repo_root":"/repo",
            "kind":"two_dot",
            "base_sha":"aaaa",
            "head_sha":"bbbb",
            "range_label":"a..b",
            "head_committed_at":"t",
            "generated_at":"t",
            "title":"diff",
            "byte_size":1
        }"#;

        let sidecar = serde_json::from_str::<Sidecar>(json).unwrap();

        assert_eq!(sidecar.renderer_version, 0);
    }

    #[test]
    fn sidecar_without_presentation_metadata_uses_default_render_options() {
        let json = r#"{
            "repo_id":"deadbeef00000000",
            "repo_name":"git-tools",
            "repo_root":"/repo",
            "kind":"two_dot",
            "base_sha":"aaaa",
            "head_sha":"bbbb",
            "range_label":"a..b",
            "head_committed_at":"t",
            "generated_at":"t",
            "title":"diff",
            "byte_size":1
        }"#;

        let sidecar = serde_json::from_str::<Sidecar>(json).unwrap();

        assert_eq!(sidecar.layout, RenderOptions::DEFAULT.layout().to_string());
        assert_eq!(
            sidecar.density,
            RenderOptions::DEFAULT.density().to_string()
        );
    }
}
