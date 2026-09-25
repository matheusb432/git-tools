//! Per-artifact metadata (sidecar) and the diff kind that keys range lookups.
use anyhow::{Context as _, bail};
pub use gtl_models::diffs::DiffKind;
use gtl_models::{
    artifacts::{ArtifactByteSize, ArtifactDiffIdentity, RepositoryStoreId},
    diffs::{CommitId, ExcludedExtensions, PinnedRange},
    paths::{ProjectName, RepositoryRoot},
    settings::ViewerLanguage,
    timestamps::MachineTimestamp,
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

/// Bumped when the renderer's HTML output changes materially so range reuse never serves an
/// artifact rendered by an older renderer.
pub const RENDERER_VERSION: u32 = 6;

/// Metadata stored alongside each artifact as `<hash>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Sidecar {
    pub(crate) repo_id: String,
    pub(crate) repo_name: String,
    pub(crate) repo_root: String,
    pub(crate) kind: DiffKind,
    pub(crate) base_sha: String,
    pub(crate) head_sha: String,
    pub(crate) range_label: String,
    pub(crate) head_committed_at: String,
    pub(crate) generated_at: String,
    pub(crate) title: String,
    pub(crate) byte_size: u64,
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
    /// Extension set in force at render time (normalized, sorted; empty =
    /// unfiltered). Defaults keep pre-exclusion sidecars readable, and their
    /// empty set correctly means "rendered without exclusions".
    #[serde(default)]
    pub(crate) excluded_extensions: Vec<String>,
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
    pub repo_name: ProjectName,
    pub repo_root: RepositoryRoot,
    pub identity: ArtifactDiffIdentity,
    pub range_label: String,
    pub head_committed_at: Option<MachineTimestamp>,
    pub generated_at: MachineTimestamp,
    pub title: String,
    pub byte_size: ArtifactByteSize,
    pub render_options: RenderOptions,
    pub theme: ArtifactThemeMetadata,
    pub language: ViewerLanguage,
    pub renderer_version: u32,
    pub excluded_extensions: ExcludedExtensions,
}

impl Sidecar {
    /// Projects one compatibility row into validated metadata.
    pub(crate) fn try_into_metadata(self) -> anyhow::Result<ArtifactMetadata> {
        let repo_id = RepositoryStoreId::try_new(self.repo_id)
            .context("sidecar has an invalid repository store ID")?;
        let repo_name =
            ProjectName::try_new(self.repo_name).context("sidecar has an invalid project name")?;
        let repo_root = RepositoryRoot::try_new(self.repo_root.into())
            .context("sidecar has an invalid repository root")?;
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
        let head_committed_at = if self.head_committed_at.is_empty() {
            None
        } else {
            Some(
                MachineTimestamp::try_from(self.head_committed_at)
                    .context("sidecar has an invalid head commit timestamp")?,
            )
        };
        let generated_at = MachineTimestamp::try_from(self.generated_at)
            .context("sidecar has an invalid generation timestamp")?;
        let language = self
            .language
            .parse::<ViewerLanguage>()
            .context("sidecar has an invalid language")?;

        Ok(ArtifactMetadata {
            repo_id,
            repo_name,
            repo_root,
            identity,
            range_label: self.range_label,
            head_committed_at,
            generated_at,
            title: self.title,
            byte_size: ArtifactByteSize::new(self.byte_size),
            render_options: RenderOptions::new(layout, density),
            theme,
            language,
            renderer_version: self.renderer_version,
            excluded_extensions: ExcludedExtensions::new(self.excluded_extensions),
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
            repo_name: metadata.repo_name.as_str().to_owned(),
            repo_root: metadata.repo_root.to_string_lossy().into_owned(),
            kind: metadata.identity.kind(),
            base_sha,
            head_sha,
            range_label: metadata.range_label.clone(),
            head_committed_at: metadata
                .head_committed_at
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            generated_at: metadata.generated_at.to_string(),
            title: metadata.title.clone(),
            byte_size: metadata.byte_size.into_inner(),
            layout: metadata.render_options.layout().to_string(),
            density: metadata.render_options.density().to_string(),
            theme,
            theme_recorded,
            language: metadata.language.as_str().to_owned(),
            renderer_version: metadata.renderer_version,
            excluded_extensions: metadata.excluded_extensions.extensions().to_vec(),
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
            repo_name: "git-tools".into(),
            repo_root: "/home/u/tools/git-tools".into(),
            kind: DiffKind::TwoDot,
            base_sha: "a".repeat(40),
            head_sha: "b".repeat(40),
            range_label: "origin/main..HEAD".into(),
            head_committed_at: "2026-06-22T10:00:00Z".into(),
            generated_at: "2026-06-22T10:01:00Z".into(),
            title: "diff".into(),
            byte_size: 1234,
            layout: DiffLayout::Split.to_string(),
            density: DiffDensity::Full.to_string(),
            theme: Some("dark".into()),
            theme_recorded: true,
            language: "pt-BR".into(),
            renderer_version: RENDERER_VERSION,
            excluded_extensions: vec![".MD".into(), "md".into()],
        }
    }

    #[test]
    fn removed_palette_preserves_history_but_cannot_match_a_cached_theme() {
        for theme in ["verdant", "noir", "light", "hearth"] {
            let mut sidecar = valid_sidecar();
            sidecar.theme = Some(theme.to_owned());
            let metadata = sidecar.try_into_metadata().unwrap();
            assert_eq!(metadata.theme, ArtifactThemeMetadata::Unrecorded);
            assert_eq!(metadata.repo_name.as_str(), "git-tools");
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
            language: "pt-BR".into(),
            renderer_version: RENDERER_VERSION,
            excluded_extensions: vec!["md".into()],
        };
        let json = serde_json::to_string(&sc).unwrap();
        assert_eq!(serde_json::from_str::<Sidecar>(&json).unwrap(), sc);
    }

    #[test]
    fn compatibility_sidecar_projects_once_into_validated_metadata() {
        let sidecar = valid_sidecar();
        let metadata = sidecar.clone().try_into_metadata().unwrap();

        assert_eq!(metadata.repo_id.as_ref(), "deadbeef00000000");
        assert!(metadata.identity.commits().is_some());
        assert_eq!(metadata.byte_size, ArtifactByteSize::new(1234));
        assert_eq!(metadata.render_options.layout(), DiffLayout::Split);
        assert_eq!(
            metadata.theme,
            ArtifactThemeMetadata::Recorded(Some(Theme::Dark))
        );
        assert_eq!(metadata.excluded_extensions.extensions(), ["md"]);
        let encoded = Sidecar::from_metadata(&metadata);
        assert_eq!(encoded.head_committed_at, sidecar.head_committed_at);
        assert_eq!(encoded.generated_at, sidecar.generated_at);
    }

    #[test]
    fn compatibility_sidecar_preserves_an_absent_head_timestamp() {
        let mut sidecar = valid_sidecar();
        sidecar.head_committed_at.clear();

        let metadata = sidecar.clone().try_into_metadata().unwrap();

        assert!(metadata.head_committed_at.is_none());
        let encoded = Sidecar::from_metadata(&metadata);
        assert_eq!(encoded.head_committed_at, sidecar.head_committed_at);
        assert_eq!(encoded.generated_at, sidecar.generated_at);
    }

    #[test]
    fn compatibility_sidecar_rejects_malformed_timestamps_at_projection() {
        let mut invalid_head = valid_sidecar();
        invalid_head.head_committed_at = "2026-06-22T10:00:00".into();
        assert!(
            invalid_head
                .try_into_metadata()
                .unwrap_err()
                .to_string()
                .contains("head commit timestamp")
        );

        let mut invalid_generation = valid_sidecar();
        invalid_generation.generated_at = "not-a-timestamp".into();
        assert!(
            invalid_generation
                .try_into_metadata()
                .unwrap_err()
                .to_string()
                .contains("generation timestamp")
        );
    }

    #[test]
    fn invalid_compatibility_identity_is_rejected_at_projection() {
        let mut sidecar = valid_sidecar();
        sidecar.repo_id = "every-repository".into();

        let error = sidecar.try_into_metadata().unwrap_err();

        assert!(error.to_string().contains("repository store ID"));
    }

    #[test]
    fn compatibility_sidecar_rejects_invalid_repository_roles_at_projection() {
        let mut unnamed = valid_sidecar();
        unnamed.repo_name.clear();
        assert!(
            unnamed
                .try_into_metadata()
                .unwrap_err()
                .to_string()
                .contains("project name")
        );

        let mut relative = valid_sidecar();
        relative.repo_root = "relative/repo".into();
        assert!(
            relative
                .try_into_metadata()
                .unwrap_err()
                .to_string()
                .contains("repository root")
        );
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
