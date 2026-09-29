//! Per-artifact metadata (sidecar) and the diff kind that keys range lookups.
use anyhow::{Context as _, bail};
pub use gtl_models::diffs::DiffKind;
use gtl_models::{
    artifacts::{ArtifactDiffIdentity, RepositoryStoreId},
    diffs::{CommitId, ExtensionFilter, ExtensionFilterMode, FileExtensions, PinnedRange},
    settings::ViewerLanguage,
    viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
};
use serde::{Deserialize, Serialize};

fn layout_default() -> String {
    RenderOptions::DEFAULT.layout().to_string()
}

fn density_default() -> String {
    RenderOptions::DEFAULT.density().to_string()
}

/// Sidecars written before artifacts were localized describe English artifacts.
fn language_default() -> String {
    ViewerLanguage::EnUs.as_str().to_owned()
}

/// Sidecars written before show-only filters hid their listed extensions.
fn extension_filter_mode_default() -> String {
    ExtensionFilterMode::Hide.as_str().to_owned()
}

/// Bumped when the renderer's HTML output changes materially so range reuse never serves an
/// artifact rendered by an older renderer.
pub const RENDERER_VERSION: u32 = 11;

/// Keys that earlier sidecars carried and nothing reads anymore.
const RETIRED_SIDECAR_KEYS: [&str; 7] = [
    "repo_name",
    "repo_root",
    "range_label",
    "head_committed_at",
    "generated_at",
    "title",
    "byte_size",
];

/// Metadata stored alongside each artifact as `<hash>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Sidecar {
    pub(crate) repo_id: String,
    pub(crate) kind: DiffKind,
    pub(crate) base_sha: String,
    pub(crate) head_sha: String,
    #[serde(default = "layout_default")]
    pub(crate) layout: String,
    #[serde(default = "density_default")]
    pub(crate) density: String,
    /// Configured renderer theme used for this artifact. `None` means the
    /// renderer selected its default theme.
    #[serde(default)]
    pub(crate) theme: Option<String>,
    /// Distinguishes current default-theme artifacts from legacy sidecars that
    /// predate theme metadata and must not satisfy range reuse.
    #[serde(default)]
    pub(crate) theme_recorded: bool,
    /// BCP 47 tag of the artifact's copy language.
    #[serde(default = "language_default")]
    pub(crate) language: String,
    /// Renderer version that produced this artifact, compared against
    /// [`RENDERER_VERSION`] to gate range reuse. Missing (pre-feature)
    /// sidecars default to `0`. Content reuse in `place` refreshes a stale
    /// stored version to the current one.
    #[serde(default)]
    pub(crate) renderer_version: u32,
    /// Extensions of the filter in force at render time (normalized, sorted;
    /// empty = unfiltered). The key predates show-only filters. Defaults keep
    /// pre-filter sidecars readable, and their empty set correctly means
    /// "rendered without a filter".
    #[serde(default, rename = "excluded_extensions")]
    pub(crate) filter_extensions: Vec<String>,
    /// Whether the filter hid or showed only its extensions.
    #[serde(default = "extension_filter_mode_default")]
    pub(crate) filter_mode: String,
}

/// Whether theme metadata was recorded by the renderer version that wrote a sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactThemeMetadata {
    Unrecorded,
    Recorded(Option<Theme>),
}

/// Validated metadata carried after the raw compatibility sidecar is decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMetadata {
    pub repo_id: RepositoryStoreId,
    pub identity: ArtifactDiffIdentity,
    pub render_options: RenderOptions,
    pub theme: ArtifactThemeMetadata,
    pub language: ViewerLanguage,
    pub renderer_version: u32,
    pub extension_filter: ExtensionFilter,
}

impl Sidecar {
    /// Decodes one stored sidecar, reporting whether it still carries retired keys that a
    /// rewrite would drop.
    pub(crate) fn from_stored_json(json: &str) -> anyhow::Result<(Self, bool)> {
        let mut document =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json)?;
        let mut retired = false;
        for key in RETIRED_SIDECAR_KEYS {
            retired |= document.remove(key).is_some();
        }
        Ok((
            serde_json::from_value(serde_json::Value::Object(document))?,
            retired,
        ))
    }

    /// Projects one compatibility row into validated metadata.
    pub(crate) fn try_into_metadata(self) -> anyhow::Result<ArtifactMetadata> {
        let repo_id = RepositoryStoreId::try_new(self.repo_id)
            .context("sidecar has an invalid repository store ID")?;
        let identity = decode_identity(self.kind, self.base_sha, self.head_sha)?;
        let layout = self
            .layout
            .parse::<DiffLayout>()
            .context("sidecar has an invalid diff layout")?;
        let density = self
            .density
            .parse::<DiffDensity>()
            .context("sidecar has an invalid diff density")?;
        let theme = if self.theme_recorded
            && !matches!(
                self.theme.as_deref(),
                Some("verdant" | "noir" | "light" | "hearth")
            ) {
            ArtifactThemeMetadata::Recorded(
                self.theme
                    .map(|theme| theme.parse::<Theme>())
                    .transpose()
                    .context("sidecar has an invalid theme")?,
            )
        } else {
            ArtifactThemeMetadata::Unrecorded
        };
        let language = self
            .language
            .parse::<ViewerLanguage>()
            .context("sidecar has an invalid language")?;
        let filter_mode = self
            .filter_mode
            .parse::<ExtensionFilterMode>()
            .context("sidecar has an invalid extension filter mode")?;

        Ok(ArtifactMetadata {
            repo_id,
            identity,
            render_options: RenderOptions::new(layout, density),
            theme,
            language,
            renderer_version: self.renderer_version,
            extension_filter: ExtensionFilter::new(
                filter_mode,
                FileExtensions::new(self.filter_extensions),
            ),
        })
    }

    /// Projects validated metadata back into the stable compatibility JSON shape.
    pub(crate) fn from_metadata(metadata: &ArtifactMetadata) -> Self {
        let (base_sha, head_sha) = metadata.identity.commits().map_or_else(
            || (String::new(), String::new()),
            |range| (range.base.to_string(), range.head.to_string()),
        );
        let (theme, theme_recorded) = match metadata.theme {
            ArtifactThemeMetadata::Unrecorded => (None, false),
            ArtifactThemeMetadata::Recorded(theme) => (theme.map(|theme| theme.to_string()), true),
        };

        Self {
            repo_id: metadata.repo_id.to_string(),
            kind: metadata.identity.kind(),
            base_sha,
            head_sha,
            layout: metadata.render_options.layout().to_string(),
            density: metadata.render_options.density().to_string(),
            theme,
            theme_recorded,
            language: metadata.language.as_str().to_owned(),
            renderer_version: metadata.renderer_version,
            filter_extensions: metadata.extension_filter.extensions().extensions().to_vec(),
            filter_mode: metadata.extension_filter.mode().as_str().to_owned(),
        }
    }
}

fn decode_identity(
    kind: DiffKind,
    base_sha: String,
    head_sha: String,
) -> anyhow::Result<ArtifactDiffIdentity> {
    let commits = match (base_sha.is_empty(), head_sha.is_empty()) {
        (true, true) => None,
        (false, false) => Some(PinnedRange {
            base: CommitId::try_from(base_sha).context("sidecar has an invalid base commit ID")?,
            head: CommitId::try_from(head_sha).context("sidecar has an invalid head commit ID")?,
        }),
        _ => bail!("sidecar commit range must contain both endpoints or neither"),
    };
    ArtifactDiffIdentity::from_parts(kind, commits)
        .context("sidecar has incompatible diff kind and commit range")
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::{DiffDensity, DiffLayout, RenderOptions};

    use super::*;

    fn valid_sidecar() -> Sidecar {
        Sidecar {
            repo_id: "deadbeef00000000".into(),
            kind: DiffKind::TwoDot,
            base_sha: "a".repeat(40),
            head_sha: "b".repeat(40),
            layout: DiffLayout::Split.to_string(),
            density: DiffDensity::Full.to_string(),
            theme: Some("dark".into()),
            theme_recorded: true,
            language: "pt-BR".into(),
            renderer_version: RENDERER_VERSION,
            filter_extensions: vec![".MD".into(), "md".into()],
            filter_mode: "only".into(),
        }
    }

    #[test]
    fn removed_palette_preserves_history_but_cannot_match_a_cached_theme() {
        for theme in ["verdant", "noir", "light", "hearth"] {
            let mut sidecar = valid_sidecar();
            sidecar.theme = Some(theme.to_owned());
            let metadata = sidecar.try_into_metadata().unwrap();
            assert_eq!(metadata.theme, ArtifactThemeMetadata::Unrecorded);
        }
    }

    #[test]
    fn sidecars_without_a_language_describe_english_artifacts() {
        let mut json = serde_json::to_value(valid_sidecar()).unwrap();
        json.as_object_mut().unwrap().remove("language");
        let sidecar: Sidecar = serde_json::from_value(json).unwrap();

        assert_eq!(
            sidecar.try_into_metadata().unwrap().language,
            ViewerLanguage::EnUs
        );
    }

    #[test]
    fn sidecars_without_a_filter_mode_hid_their_extensions() {
        let mut json = serde_json::to_value(valid_sidecar()).unwrap();
        json.as_object_mut().unwrap().remove("filter_mode");
        let sidecar: Sidecar = serde_json::from_value(json).unwrap();

        assert_eq!(
            sidecar.try_into_metadata().unwrap().extension_filter,
            ExtensionFilter::new(ExtensionFilterMode::Hide, FileExtensions::new(["md"]))
        );
    }

    #[test]
    fn sidecar_round_trips_through_json() {
        let sidecar = valid_sidecar();
        let json = serde_json::to_string(&sidecar).unwrap();

        assert_eq!(Sidecar::from_stored_json(&json).unwrap(), (sidecar, false));
    }

    #[test]
    fn stored_sidecars_shed_retired_keys() {
        let mut json = serde_json::to_value(valid_sidecar()).unwrap();
        let document = json.as_object_mut().unwrap();
        for (key, value) in [
            ("repo_name", serde_json::json!("git-tools")),
            ("repo_root", serde_json::json!("/repo")),
            ("range_label", serde_json::json!("main..HEAD")),
            (
                "head_committed_at",
                serde_json::json!("2026-06-22T10:00:00Z"),
            ),
            ("generated_at", serde_json::json!("2026-06-22T10:01:00Z")),
            ("title", serde_json::json!("diff")),
            ("byte_size", serde_json::json!(1234)),
        ] {
            document.insert(key.to_owned(), value);
        }

        let (sidecar, has_retired_keys) = Sidecar::from_stored_json(&json.to_string()).unwrap();

        assert!(has_retired_keys);
        assert_eq!(sidecar, valid_sidecar());
        assert_eq!(
            serde_json::to_value(&sidecar).unwrap(),
            serde_json::to_value(valid_sidecar()).unwrap()
        );
    }

    #[test]
    fn compatibility_sidecar_projects_once_into_validated_metadata() {
        let sidecar = valid_sidecar();
        let metadata = sidecar.clone().try_into_metadata().unwrap();

        assert_eq!(metadata.repo_id.as_ref(), "deadbeef00000000");
        assert!(metadata.identity.commits().is_some());
        assert_eq!(metadata.render_options.layout(), DiffLayout::Split);
        assert_eq!(
            metadata.theme,
            ArtifactThemeMetadata::Recorded(Some(Theme::Dark))
        );
        assert_eq!(
            metadata.extension_filter,
            ExtensionFilter::new(ExtensionFilterMode::Only, FileExtensions::new(["md"]))
        );
        let rewritten = Sidecar::from_metadata(&metadata);
        assert_eq!(rewritten.filter_extensions, ["md"]);
        assert_eq!(rewritten.filter_mode, "only");
    }

    #[test]
    fn invalid_compatibility_identity_is_rejected_at_projection() {
        let mut sidecar = valid_sidecar();
        sidecar.repo_id = "every-repository".into();

        let error = sidecar.try_into_metadata().unwrap_err();

        assert!(error.to_string().contains("repository store ID"));
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
