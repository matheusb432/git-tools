use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerActiveView, ViewerDiffFileId};

use crate::{
    entities::diffs::{ClientDiffWorkspace, static_diff_workspace},
    shared::ui::{Button, ButtonSize, ButtonVariant},
    views::diffs::ArtifactDiffWorkspace,
};

const STATIC_ARTIFACT_ENHANCEMENT_SCRIPT: &str = include_str!("artifact.js");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticArtifactView {
    view: ViewerActiveView,
    workspace: ClientDiffWorkspace,
}

impl StaticArtifactView {
    pub fn try_new(
        view: ViewerActiveView,
        sources: Vec<StaticArtifactFileSource>,
    ) -> Result<Self, StaticArtifactViewError> {
        let expected = view
            .files
            .iter()
            .map(|file| file.id.clone())
            .collect::<HashSet<_>>();
        let mut source_lines = HashMap::with_capacity(sources.len());
        for source in sources {
            if !expected.contains(&source.file) {
                return Err(StaticArtifactViewError::UnknownFileSource {
                    file: source.file.as_str().to_owned(),
                });
            }
            if source_lines
                .insert(source.file.clone(), source.lines)
                .is_some()
            {
                return Err(StaticArtifactViewError::DuplicateFileSource {
                    file: source.file.as_str().to_owned(),
                });
            }
        }
        let files = view
            .files
            .iter()
            .cloned()
            .map(|summary| {
                let lines = source_lines.remove(&summary.id).ok_or_else(|| {
                    StaticArtifactViewError::MissingFileSource {
                        file: summary.id.as_str().to_owned(),
                    }
                })?;
                Ok((summary, lines))
            })
            .collect::<Result<Vec<_>, StaticArtifactViewError>>()?;
        let workspace = static_diff_workspace(view.identity, files);
        Ok(Self { view, workspace })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticArtifactFileSource {
    pub file: ViewerDiffFileId,
    pub lines: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StaticArtifactViewError {
    #[error("artifact source contains unknown diff file {file}")]
    UnknownFileSource { file: String },
    #[error("artifact source contains diff file {file} more than once")]
    DuplicateFileSource { file: String },
    #[error("artifact source is missing diff file {file}")]
    MissingFileSource { file: String },
}

#[must_use]
pub fn render_static_artifact_body(views: Vec<StaticArtifactView>) -> String {
    dioxus_ssr::render_element(rsx! {
        StaticArtifactDocument { views }
    })
}

#[must_use]
pub const fn static_artifact_enhancement_script() -> &'static str {
    STATIC_ARTIFACT_ENHANCEMENT_SCRIPT
}

#[component]
fn StaticArtifactDocument(views: Vec<StaticArtifactView>) -> Element {
    if views.is_empty() {
        return rsx! {
            main { class: "grid h-screen place-content-center bg-bg px-5 text-center text-ink",
                h1 { class: "font-semibold", "No diffs in this artifact" }
            }
        };
    }

    let has_tabs = views.len() > 1;
    rsx! {
        main {
            class: "flex h-screen min-h-0 flex-col overflow-hidden bg-bg text-ink",
            "data-gtl-artifact-ready": "true",
            if has_tabs {
                nav {
                    class: "flex flex-none items-center gap-1.5 overflow-x-auto border-b border-line bg-surface-2 px-3 py-2.5",
                    role: "tablist",
                    aria_label: "Subrepo diffs",
                    for (index, artifact) in views.iter().enumerate() {
                        {
                            let tab_id = artifact.view.identity.tab_id;
                            let button_id = format!("artifact-tab-{tab_id}");
                            let panel_id = format!("artifact-panel-{tab_id}");
                            rsx! {
                                Button {
                                    key: "{artifact.view.identity.tab_id}",
                                    id: button_id,
                                    class: "max-w-[280px] overflow-hidden text-ellipsis",
                                    size: ButtonSize::Small,
                                    variant: if index == 0 { ButtonVariant::Pressed } else { ButtonVariant::Outline },
                                    role: "tab",
                                    aria_selected: (index == 0).to_string(),
                                    aria_controls: panel_id,
                                    tabindex: if index == 0 { "0" } else { "-1" },
                                    "data-gtl-artifact-tab": index,
                                    "data-gtl-action": "select-view",
                                    "data-gtl-view": tab_id.to_string(),
                                    "data-gtl-selected-classes": ButtonVariant::Pressed.classes(),
                                    "data-gtl-unselected-classes": ButtonVariant::Outline.classes(),
                                    "{artifact.view.repository_name}"
                                }
                            }
                        }
                    }
                }
            }
            for (index, artifact) in views.into_iter().enumerate() {
                {
                    let tab_id = artifact.view.identity.tab_id;
                    let panel_id = format!("artifact-panel-{tab_id}");
                    let button_id = format!("artifact-tab-{tab_id}");
                    let repository_name = artifact.view.repository_name.to_string();
                    rsx! {
                        section {
                            key: "{artifact.view.identity.tab_id}",
                            id: panel_id,
                            class: "min-h-0 flex-1",
                            role: "tabpanel",
                            aria_label: (!has_tabs).then_some(repository_name),
                            aria_labelledby: has_tabs.then_some(button_id),
                            hidden: index != 0,
                            "data-gtl-artifact-panel": index,
                            "data-gtl-view-panel": "",
                            "data-gtl-view": tab_id.to_string(),
                            ArtifactDiffWorkspace { view: artifact.view, workspace: artifact.workspace }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        diffs::DiffLineCount,
        git::{BranchName, GitHead, GitRevision},
        viewer::{ViewerRangeGeneration, ViewerSelectionGeneration},
    };
    use gtl_wire::viewer::{
        ViewerActiveView, ViewerCommandLine, ViewerCommitSelection, ViewerDiffDensity,
        ViewerDiffLayout, ViewerFileStatus, ViewerFileSummary, ViewerFooter, ViewerRenderOptions,
        ViewerViewIdentity,
    };

    use super::*;
    use crate::test_support::{
        absolute_file_path, project_name, repository_relative_path, viewer_tab_id,
    };

    #[test]
    fn enhancement_script_only_mutates_existing_document_state() {
        let script = static_artifact_enhancement_script();

        assert_eq!(script.matches("addEventListener").count(), 3);
        assert!(script.contains("data-gtl-copy-line"));
        assert!(script.contains("data-gtl-action='select-view'"));
        for forbidden in [
            "innerHTML",
            "DOMParser",
            "JSON.parse",
            "fetch(",
            "XMLHttpRequest",
            "WebSocket",
            "eval(",
            "import(",
            "WebAssembly",
            "MutationObserver",
            "customElements",
            "localStorage",
            "sessionStorage",
        ] {
            assert!(
                !script.contains(forbidden),
                "enhancer retained forbidden runtime behavior: {forbidden}"
            );
        }
    }

    #[test]
    fn static_view_matches_sources_by_typed_file_id() -> crate::test_support::TestResult {
        let view = test_view(2)?;
        let first = view.files[0].id.clone();
        let second = view.files[1].id.clone();
        let artifact = StaticArtifactView::try_new(
            view,
            vec![
                StaticArtifactFileSource {
                    file: second,
                    lines: vec!["@@ -0,0 +1 @@".to_owned(), "+second-marker".to_owned()],
                },
                StaticArtifactFileSource {
                    file: first,
                    lines: vec!["@@ -0,0 +1 @@".to_owned(), "+first-marker".to_owned()],
                },
            ],
        )?;
        let html = render_static_artifact_body(vec![artifact]);

        let first_position = html
            .find("first-marker")
            .ok_or_else(|| std::io::Error::other("missing first source marker"))?;
        let second_position = html
            .find("second-marker")
            .ok_or_else(|| std::io::Error::other("missing second source marker"))?;
        assert!(first_position < second_position);
        Ok(())
    }

    #[test]
    fn static_view_rejects_unknown_duplicate_and_missing_sources() -> crate::test_support::TestResult
    {
        let empty = test_view(0)?;
        let orphan = ViewerDiffFileId::for_index(0);
        assert_eq!(
            StaticArtifactView::try_new(
                empty,
                vec![StaticArtifactFileSource {
                    file: orphan.clone(),
                    lines: vec!["+orphan".to_owned()],
                }],
            ),
            Err(StaticArtifactViewError::UnknownFileSource {
                file: orphan.as_str().to_owned(),
            })
        );

        let one_file = test_view(1)?;
        let file = one_file.files[0].id.clone();
        assert_eq!(
            StaticArtifactView::try_new(
                one_file.clone(),
                vec![
                    StaticArtifactFileSource {
                        file: file.clone(),
                        lines: Vec::new(),
                    },
                    StaticArtifactFileSource {
                        file: file.clone(),
                        lines: Vec::new(),
                    },
                ],
            ),
            Err(StaticArtifactViewError::DuplicateFileSource {
                file: file.as_str().to_owned(),
            })
        );
        assert_eq!(
            StaticArtifactView::try_new(one_file, Vec::new()),
            Err(StaticArtifactViewError::MissingFileSource {
                file: file.as_str().to_owned(),
            })
        );
        Ok(())
    }

    fn test_view(file_count: usize) -> crate::test_support::TestResult<ViewerActiveView> {
        let files = (0..file_count)
            .map(|index| {
                Ok(ViewerFileSummary {
                    id: ViewerDiffFileId::for_index(index),
                    path: repository_relative_path(&format!("src/file_{index}.rs"))?,
                    absolute_path: absolute_file_path(format!("/repo/src/file_{index}.rs"))?,
                    anchor_id: format!("file-{index}"),
                    added: DiffLineCount::new(1),
                    removed: DiffLineCount::default(),
                    status: ViewerFileStatus::Added,
                    can_open_in_editor: true,
                    initially_expanded: true,
                })
            })
            .collect::<crate::test_support::TestResult<Vec<_>>>()?;
        Ok(ViewerActiveView {
            identity: ViewerViewIdentity {
                tab_id: viewer_tab_id(1)?,
                range_generation: ViewerRangeGeneration::new(1),
                selection_generation: ViewerSelectionGeneration::default(),
                render_options: ViewerRenderOptions {
                    layout: ViewerDiffLayout::Unified,
                    density: ViewerDiffDensity::Compact,
                },
            },
            title: "diff".to_owned(),
            repository_name: project_name("repo")?,
            branch: GitHead::Branch(BranchName::main()),
            upstream: GitRevision::main(),
            command: ViewerCommandLine {
                lead: "git diff ".to_owned(),
                range: "HEAD~1..HEAD".to_owned(),
                trail: String::new(),
            },
            files,
            commits_label: "0 commits".to_owned(),
            commits: Vec::new(),
            commit_selection: ViewerCommitSelection::None,
            footer: ViewerFooter {
                command: "git diff".to_owned(),
            },
            exclusions: None,
        })
    }
}
