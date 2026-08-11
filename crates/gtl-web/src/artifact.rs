use std::cell::RefCell;

use dioxus::prelude::*;
use gtl_contracts::viewer::{
    VIEWER_ARTIFACT_MANIFEST_ID, VIEWER_ARTIFACT_SYNTAX_ID, ViewerArtifactManifest,
};
use gtl_parser::SyntaxCatalog;

use crate::{
    artifact_asset::{
        self, ArtifactAssetError, ArtifactAssetKind, MANIFEST_MAX_BYTES, SYNTAX_PACK_MAX_BYTES,
    },
    entities::diffs::theme_value,
    shared::{
        browser,
        ui::{Button, ButtonSize, ButtonVariant},
    },
    views::diffs::ArtifactDiffWorkspace,
};

thread_local! {
    static ARTIFACT_SYNTAX_CATALOG: RefCell<Option<SyntaxCatalog>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactStartupError {
    ManifestAsset(ArtifactAssetError),
    InvalidManifest,
    SyntaxAsset(ArtifactAssetError),
    InvalidSyntaxCatalog,
}

impl ArtifactStartupError {
    fn message(self) -> &'static str {
        match self {
            Self::ManifestAsset(error) => error.message(ArtifactAssetKind::Manifest),
            Self::InvalidManifest => "This artifact contains an invalid diff manifest.",
            Self::SyntaxAsset(error) => error.message(ArtifactAssetKind::SyntaxCatalog),
            Self::InvalidSyntaxCatalog => "This artifact contains an invalid syntax catalog.",
        }
    }
}

#[component]
pub(crate) fn ArtifactApp() -> Element {
    let startup = use_resource(load_startup);
    let mut active_view = use_signal(|| 0_usize);

    use_effect(move || {
        if let Some(Ok(manifest)) = &*startup.read() {
            browser::apply_theme(theme_value(manifest.theme));
        }
    });

    let loaded = startup.read();
    rsx! {
        match &*loaded {
            None => rsx! {
                main {
                    class: "grid h-screen place-content-center bg-bg px-5 text-center text-ink",
                    role: "status",
                    "Opening diff artifact"
                }
            },
            Some(Err(error)) => rsx! {
                main {
                    class: "grid h-screen place-content-center bg-bg px-5 text-center text-ink",
                    role: "alert",
                    h1 { class: "font-semibold", "Unable to open diff artifact" }
                    p { class: "mt-1 text-ink-2", "{error.message()}" }
                }
            },
            Some(Ok(manifest)) if manifest.views.is_empty() => rsx! {
                main { class: "grid h-screen place-content-center bg-bg px-5 text-center text-ink",
                    h1 { class: "font-semibold", "No diffs in this artifact" }
                }
            },
            Some(Ok(manifest)) => {
                let selected = active_view().min(manifest.views.len() - 1);
                let view = manifest.views[selected].clone();
                rsx! {
                    document::Title { "{manifest.title}" }
                    main {
                        class: "flex h-screen min-h-0 flex-col overflow-hidden bg-bg text-ink",
                        "data-gtl-artifact-ready": "true",
                        if manifest.views.len() > 1 {
                            nav {
                                class: "flex flex-none items-center gap-1.5 overflow-x-auto border-b border-line bg-surface-2 px-3 py-2.5",
                                role: "tablist",
                                aria_label: "Subrepo diffs",
                                for (index, candidate) in manifest.views.iter().enumerate() {
                                    Button {
                                        key: "{candidate.identity.tab_id}",
                                        class: "max-w-[280px] overflow-hidden text-ellipsis",
                                        size: ButtonSize::Small,
                                        variant: if index == selected { ButtonVariant::Pressed } else { ButtonVariant::Outline },
                                        role: "tab",
                                        aria_selected: (index == selected).to_string(),
                                        onclick: move |_| active_view.set(index),
                                        "{candidate.repository_name}"
                                    }
                                }
                            }
                        }
                        div { class: "min-h-0 flex-1",
                            ArtifactDiffWorkspace { key: "{view.identity.tab_id}", view }
                        }
                    }
                }
            }
        }
    }
}

async fn load_startup() -> Result<ViewerArtifactManifest, ArtifactStartupError> {
    let manifest_bytes = artifact_asset::load(VIEWER_ARTIFACT_MANIFEST_ID, MANIFEST_MAX_BYTES)
        .await
        .map_err(ArtifactStartupError::ManifestAsset)?;
    let manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| ArtifactStartupError::InvalidManifest)?;
    artifact_asset::remove(VIEWER_ARTIFACT_MANIFEST_ID);

    let syntax_bytes = artifact_asset::load(VIEWER_ARTIFACT_SYNTAX_ID, SYNTAX_PACK_MAX_BYTES)
        .await
        .map_err(ArtifactStartupError::SyntaxAsset)?;
    let syntax_catalog = SyntaxCatalog::from_uncompressed_pack(&syntax_bytes)
        .map_err(|_| ArtifactStartupError::InvalidSyntaxCatalog)?;
    artifact_asset::remove(VIEWER_ARTIFACT_SYNTAX_ID);
    ARTIFACT_SYNTAX_CATALOG.with(|catalog| catalog.replace(Some(syntax_catalog)));
    Ok(manifest)
}

pub(crate) fn syntax_catalog() -> Option<SyntaxCatalog> {
    ARTIFACT_SYNTAX_CATALOG.with(|catalog| catalog.borrow().clone())
}
