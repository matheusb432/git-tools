use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use gtl_wire::viewer::{ViewerActiveView, ViewerDiffFileId, ViewerFileRows, ViewerFileSummary};

use crate::{
    entities::diffs::{ClientDiffWorkspace, static_diff_workspace},
    shared::{
        i18n::{t, use_language_provider},
        ui::{Button, ButtonSize, ButtonVariant, ScrollArea, scroll_area::ScrollAreaVariant},
    },
    views::diffs::ArtifactDiffWorkspace,
};

const STATIC_ARTIFACT_ENHANCEMENT_SCRIPT: &str =
    include_str!(concat!(env!("OUT_DIR"), "/artifact.min.js"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticArtifactView {
    view: ViewerActiveView,
    workspace: ClientDiffWorkspace,
}

impl StaticArtifactView {
    pub fn try_new(
        view: ViewerActiveView,
        files: Vec<StaticArtifactFileRows>,
    ) -> Result<Self, StaticArtifactViewError> {
        let expected = view
            .files
            .iter()
            .map(|file| file.id.clone())
            .collect::<HashSet<_>>();
        let mut file_rows = collect_file_rows(&expected, files)?;
        let files = view
            .files
            .iter()
            .cloned()
            .map(|summary| take_file_rows(&mut file_rows, summary))
            .collect::<Result<Vec<_>, StaticArtifactViewError>>()?;
        let workspace = static_diff_workspace(view.identity, files);
        Ok(Self { view, workspace })
    }
}

fn collect_file_rows(
    expected: &HashSet<ViewerDiffFileId>,
    files: Vec<StaticArtifactFileRows>,
) -> Result<HashMap<ViewerDiffFileId, ViewerFileRows>, StaticArtifactViewError> {
    let mut file_rows = HashMap::with_capacity(files.len());
    for file in files {
        insert_file_rows(expected, &mut file_rows, file)?;
    }
    Ok(file_rows)
}

fn insert_file_rows(
    expected: &HashSet<ViewerDiffFileId>,
    file_rows: &mut HashMap<ViewerDiffFileId, ViewerFileRows>,
    file: StaticArtifactFileRows,
) -> Result<(), StaticArtifactViewError> {
    if !expected.contains(&file.file) {
        return Err(StaticArtifactViewError::Unknown {
            file: file.file.as_str().to_owned(),
        });
    }
    if file_rows.insert(file.file.clone(), file.rows).is_some() {
        return Err(StaticArtifactViewError::Duplicate {
            file: file.file.as_str().to_owned(),
        });
    }
    Ok(())
}

fn take_file_rows(
    file_rows: &mut HashMap<ViewerDiffFileId, ViewerFileRows>,
    summary: ViewerFileSummary,
) -> Result<(ViewerFileSummary, ViewerFileRows), StaticArtifactViewError> {
    let rows = file_rows
        .remove(&summary.id)
        .ok_or_else(|| StaticArtifactViewError::Missing {
            file: summary.id.as_str().to_owned(),
        })?;
    Ok((summary, rows))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticArtifactFileRows {
    pub file: ViewerDiffFileId,
    pub rows: ViewerFileRows,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StaticArtifactViewError {
    #[error("artifact rows contain unknown diff file {file}")]
    Unknown { file: String },
    #[error("artifact rows contain diff file {file} more than once")]
    Duplicate { file: String },
    #[error("artifact rows are missing diff file {file}")]
    Missing { file: String },
}

/// Renders the artifact body with its copy in `language`.
#[must_use]
pub fn render_static_artifact_body(
    views: Vec<StaticArtifactView>,
    language: ViewerLanguage,
) -> String {
    dioxus_ssr::render_element(rsx! {
        StaticArtifactDocument { views, language }
    })
}

#[must_use]
pub const fn static_artifact_enhancement_script() -> &'static str {
    STATIC_ARTIFACT_ENHANCEMENT_SCRIPT
}

#[component]
fn StaticArtifactDocument(views: Vec<StaticArtifactView>, language: ViewerLanguage) -> Element {
    let provided_language = use_signal(|| language);
    use_language_provider(provided_language.into());
    if views.is_empty() {
        return rsx! {
            main { class: "artifact-empty h-screen px-5",
                h1 { class: "font-semibold", {t!(language, "artifact-empty")} }
            }
        };
    }

    let has_tabs = views.len() > 1;
    rsx! {
        main {
            class: "viewer-shell h-screen min-h-0",
            "data-gtl-artifact-ready": "true",
            // The enhancer writes these labels, so feedback follows the artifact's language.
            "data-gtl-label-expand-all": t!(language, "titlebar-expand-all"),
            "data-gtl-label-collapse-all": t!(language, "titlebar-collapse-all"),
            "data-gtl-label-copied": t!(language, "copy-copied"),
            "data-gtl-label-copy-failed": t!(language, "copy-failed"),
            "data-gtl-label-copied-context": t!(language, "copy-context"),
            "data-gtl-label-copied-context-lines": t!(language, "copy-context-lines", lines = "{lines}"),
            if has_tabs {
                ScrollArea {
                    variant: ScrollAreaVariant::Rail,
                    class: "flex-none border-b border-line bg-surface-2",
                    role: "tablist",
                    aria_label: t!(language, "artifact-subrepo-diffs"),
                    div { class: "artifact-tabs gap-1.5 px-3 py-2.5",
                        for (index, artifact) in views.iter().enumerate() {
                            {
                                let tab_id = artifact.view.identity.tab_id;
                                let button_id = format!("artifact-tab-{tab_id}");
                                let panel_id = format!("artifact-panel-{tab_id}");
                                rsx! {
                                    Button {
                                        key: "{artifact.view.identity.tab_id}",
                                        id: button_id,
                                        class: "artifact-tab",
                                        size: ButtonSize::Small,
                                        variant: ButtonVariant::Bare,
                                        role: "tab",
                                        aria_selected: (index == 0).to_string(),
                                        aria_controls: panel_id,
                                        tabindex: if index == 0 { "0" } else { "-1" },
                                        "data-gtl-artifact-tab": index,
                                        "data-gtl-action": "select-view",
                                        "data-gtl-view": tab_id.to_string(),
                                        "{artifact.view.repository_name}"
                                    }
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
        ViewerActiveView, ViewerCodeLine, ViewerCodeSpan, ViewerCommandLine, ViewerCommitSelection,
        ViewerDiffDensity, ViewerDiffLayout, ViewerFileRows, ViewerFileStatus, ViewerFileSummary,
        ViewerFooter, ViewerRenderOptions, ViewerRows, ViewerUnifiedRow, ViewerUnifiedSourceRow,
        ViewerViewIdentity,
    };

    use super::*;
    use crate::test_support::{
        absolute_file_path, project_name, repository_relative_path, viewer_tab_id,
    };

    #[test]
    fn enhancement_script_only_mutates_existing_document_state() {
        let script = static_artifact_enhancement_script();

        assert_eq!(script.matches("addEventListener").count(), 17);
        assert!(script.contains("root.addEventListener(\"copy\""));
        assert!(script.contains("root.addEventListener(\"mouseover\""));
        assert!(script.contains("data-gtl-hover-popover-target"));
        assert!(script.contains("data-gtl-copy-line"));
        assert!(!script.contains("function codePayload"));
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
    fn static_view_matches_rows_by_typed_file_id() -> crate::test_support::TestResult {
        let view = test_view(2)?;
        let first = view.files[0].id.clone();
        let second = view.files[1].id.clone();
        let artifact = StaticArtifactView::try_new(
            view,
            vec![
                StaticArtifactFileRows {
                    file: second,
                    rows: file_rows("second-marker"),
                },
                StaticArtifactFileRows {
                    file: first,
                    rows: file_rows("first-marker"),
                },
            ],
        )?;
        let html = render_static_artifact_body(vec![artifact], ViewerLanguage::EnUs);

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
    fn static_view_rejects_unknown_duplicate_and_missing_rows() -> crate::test_support::TestResult {
        let empty = test_view(0)?;
        let orphan = ViewerDiffFileId::for_index(0);
        assert_eq!(
            StaticArtifactView::try_new(
                empty,
                vec![StaticArtifactFileRows {
                    file: orphan.clone(),
                    rows: file_rows("orphan"),
                }],
            ),
            Err(StaticArtifactViewError::Unknown {
                file: orphan.as_str().to_owned(),
            })
        );

        let one_file = test_view(1)?;
        let file = one_file.files[0].id.clone();
        assert_eq!(
            StaticArtifactView::try_new(
                one_file.clone(),
                vec![
                    StaticArtifactFileRows {
                        file: file.clone(),
                        rows: file_rows("first"),
                    },
                    StaticArtifactFileRows {
                        file: file.clone(),
                        rows: file_rows("second"),
                    },
                ],
            ),
            Err(StaticArtifactViewError::Duplicate {
                file: file.as_str().to_owned(),
            })
        );
        assert_eq!(
            StaticArtifactView::try_new(one_file, Vec::new()),
            Err(StaticArtifactViewError::Missing {
                file: file.as_str().to_owned(),
            })
        );
        Ok(())
    }

    fn file_rows(text: &str) -> ViewerFileRows {
        ViewerFileRows {
            rows: ViewerRows::Unified(vec![ViewerUnifiedRow::Added(ViewerUnifiedSourceRow {
                old_line_number: None,
                new_line_number: Some(1),
                code: ViewerCodeLine {
                    text: text.to_owned(),
                    spans: vec![ViewerCodeSpan {
                        byte_start: 0,
                        byte_end: text.len(),
                        syntax_class: None,
                        changed: false,
                    }],
                    omitted_character_count: None,
                },
            })]),
            line_number_digits: 1,
        }
    }

    fn test_view(file_count: usize) -> crate::test_support::TestResult<ViewerActiveView> {
        let files = (0..file_count)
            .map(|index| {
                Ok(ViewerFileSummary {
                    source_id: None,
                    id: ViewerDiffFileId::for_index(index),
                    path: repository_relative_path(&format!("src/file_{index}.rs"))?,
                    absolute_path: absolute_file_path(format!("/repo/src/file_{index}.rs"))?,
                    anchor_id: format!("file-{index}"),
                    added: DiffLineCount::new(1),
                    removed: DiffLineCount::default(),
                    status: ViewerFileStatus::Added,
                    can_open_in_editor: true,
                    initially_expanded: true,
                    row_count: 1,
                })
            })
            .collect::<crate::test_support::TestResult<Vec<_>>>()?;
        Ok(ViewerActiveView {
            modified_files: false,
            row_source: gtl_wire::viewer::ViewerRowSourceState::Ready,
            content_id: gtl_wire::viewer::ViewerRowContentId::from_digest([0; 32]),
            identity: ViewerViewIdentity {
                tab_id: viewer_tab_id(1)?,
                range_generation: ViewerRangeGeneration::new(1),
                selection_generation: ViewerSelectionGeneration::default(),
                render_options: ViewerRenderOptions {
                    wrap_lines: false,
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
            commit_count: 0,
            commits: Vec::new(),
            commit_selection: ViewerCommitSelection::None,
            footer: ViewerFooter {
                command: "git diff".to_owned(),
            },
            exclusions: None,
        })
    }
}
