use base64::{Engine as _, engine::general_purpose::STANDARD};
use dioxus::prelude::*;
use gtl_contracts::viewer::{VIEWER_ARTIFACT_MANIFEST_ID, ViewerArtifactManifest};

use crate::{
    entities::diffs::theme_value,
    shared::{
        browser,
        ui::{Button, ButtonSize, ButtonVariant},
    },
    views::diffs::ArtifactDiffWorkspace,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactManifestError {
    Missing,
    Invalid,
}

impl ArtifactManifestError {
    const fn message(self) -> &'static str {
        match self {
            Self::Missing => "This artifact does not contain its diff manifest.",
            Self::Invalid => "This artifact contains an invalid diff manifest.",
        }
    }
}

#[component]
pub(crate) fn ArtifactApp() -> Element {
    let manifest = use_signal(load_manifest);
    let mut active_view = use_signal(|| 0_usize);

    use_effect(move || {
        if let Ok(manifest) = &*manifest.read() {
            browser::apply_theme(theme_value(manifest.theme));
        }
    });

    let loaded = manifest.read();
    rsx! {
        match &*loaded {
            Err(error) => rsx! {
                main {
                    class: "grid h-screen place-content-center bg-bg px-5 text-center text-ink",
                    role: "alert",
                    h1 { class: "font-semibold", "Unable to open diff artifact" }
                    p { class: "mt-1 text-ink-2", "{error.message()}" }
                }
            },
            Ok(manifest) if manifest.views.is_empty() => rsx! {
                main { class: "grid h-screen place-content-center bg-bg px-5 text-center text-ink",
                    h1 { class: "font-semibold", "No diffs in this artifact" }
                }
            },
            Ok(manifest) => {
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

fn load_manifest() -> Result<ViewerArtifactManifest, ArtifactManifestError> {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or(ArtifactManifestError::Missing)?;
    let encoded = document
        .get_element_by_id(VIEWER_ARTIFACT_MANIFEST_ID)
        .and_then(|element| element.text_content())
        .ok_or(ArtifactManifestError::Missing)?;
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|_| ArtifactManifestError::Invalid)?;
    serde_json::from_slice(&bytes).map_err(|_| ArtifactManifestError::Invalid)
}
