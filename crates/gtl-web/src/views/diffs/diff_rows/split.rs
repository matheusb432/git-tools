use dioxus::prelude::*;

use super::{
    HEADER_CODE_CLASSES, HeaderTone,
    content::{ChangedTextTone, CodeCellContent, CodeLineSource, non_breaking_if_empty},
};
use crate::entities::diffs::{
    ClientDiffFile, ClientDiffFileStoreExt, ClientDiffRowsStoreExt, ViewerSplitRow,
};

const SPLIT_CODE_CELL_CLASSES: &str = "min-w-0 border-0 px-3 text-sm text-code whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 print:text-[#111]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitSide {
    Old,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitCellPresentation {
    OldContext,
    NewContext,
    Removed,
    Added,
}

#[component]
pub(crate) fn SplitDiffRowBatch(
    file: ReadStore<ClientDiffFile>,
    batch_index: usize,
    artifact_enhancement: bool,
) -> Element {
    let rows = file.rows().split().index(batch_index);
    rsx! {
        SplitRows { rows, artifact_enhancement, first_row: batch_index * 64 }
    }
}

#[component]
fn SplitRows(
    rows: ReadStore<Vec<ViewerSplitRow>>,
    artifact_enhancement: bool,
    #[props(default)] first_row: usize,
) -> Element {
    if artifact_enhancement {
        return rsx! {
            for (index, row) in rows.iter().enumerate() {
                SplitDiffRowView {
                    key: "{index}",
                    row,
                    artifact_enhancement,
                    row_index: first_row + index,
                }
            }
        };
    }
    rsx! {
        for (index, row) in rows.iter().enumerate() {
            div {
                key: "{index}",
                "data-row-index": (first_row + index).to_string(),
                SplitDiffRowView {
                    row,
                    artifact_enhancement,
                    row_index: first_row + index,
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SplitRowPresentation {
    Header {
        tone: HeaderTone,
        text: String,
    },
    Context {
        old_line_number: u32,
        new_line_number: u32,
    },
    Pair {
        old_line_number: Option<u32>,
        new_line_number: Option<u32>,
    },
}

#[component]
fn SplitDiffRowView(
    row: ReadStore<ViewerSplitRow>,
    artifact_enhancement: bool,
    row_index: usize,
) -> Element {
    #[cfg(feature = "desktop")]
    crate::views::diffs::presentation::use_diff_row(row_index);
    #[cfg(not(feature = "desktop"))]
    let _ = row_index;
    let presentation = {
        let row = row.read();
        match &*row {
            ViewerSplitRow::Meta(text) => SplitRowPresentation::Header {
                tone: HeaderTone::Meta,
                text: non_breaking_if_empty(text),
            },
            ViewerSplitRow::Hunk(text) => SplitRowPresentation::Header {
                tone: HeaderTone::Hunk,
                text: text.clone(),
            },
            ViewerSplitRow::Context {
                old_line_number,
                new_line_number,
                ..
            } => SplitRowPresentation::Context {
                old_line_number: *old_line_number,
                new_line_number: *new_line_number,
            },
            ViewerSplitRow::Pair { old, new } => SplitRowPresentation::Pair {
                old_line_number: old.as_ref().map(|cell| cell.line_number),
                new_line_number: new.as_ref().map(|cell| cell.line_number),
            },
        }
    };

    match presentation {
        SplitRowPresentation::Header { tone, text } => rsx! {
            SplitHeaderRow { tone, text }
        },
        SplitRowPresentation::Context {
            old_line_number,
            new_line_number,
        } => rsx! {
            SplitRowShell {
                SplitContextCell {
                    row,
                    side: SplitSide::Old,
                    line_number: old_line_number,
                    artifact_enhancement,
                    copy_line_number: None,
                }
                SplitContextCell {
                    row,
                    side: SplitSide::New,
                    line_number: new_line_number,
                    artifact_enhancement,
                    copy_line_number: Some(new_line_number),
                }
            }
        },
        SplitRowPresentation::Pair {
            old_line_number,
            new_line_number,
        } => rsx! {
            SplitRowShell {
                SplitCell {
                    row,
                    line_number: old_line_number,
                    side: SplitSide::Old,
                    artifact_enhancement,
                    copy_line_number: None,
                }
                SplitCell {
                    row,
                    line_number: new_line_number,
                    side: SplitSide::New,
                    artifact_enhancement,
                    copy_line_number: new_line_number,
                }
            }
        },
    }
}

#[component]
fn SplitHeaderRow(tone: HeaderTone, text: String) -> Element {
    let (shell_tone_classes, code_tone_classes) = match tone {
        HeaderTone::Meta => ("opacity-60", ""),
        HeaderTone::Hunk => ("bg-sunk", "font-semibold"),
    };
    rsx! {
        div {
            class: "grid grid-cols-[minmax(0,1fr)] items-stretch whitespace-normal",
            class: "{shell_tone_classes}",
            "data-gtl-diff-row": "",
            code { class: "{HEADER_CODE_CLASSES}", class: "{code_tone_classes}", "{text}" }
        }
    }
}

#[component]
fn SplitRowShell(children: Element) -> Element {
    rsx! {
        div {
            class: "grid grid-cols-[44px_minmax(0,1fr)_44px_minmax(0,1fr)] items-stretch whitespace-normal tablet:grid-cols-[44px_minmax(0,1fr)] mobile:grid-cols-[30px_minmax(0,1fr)]",
            "data-gtl-diff-row": "",
            {children}
        }
    }
}

#[component]
fn SplitContextCell(
    row: ReadStore<ViewerSplitRow>,
    side: SplitSide,
    line_number: u32,
    artifact_enhancement: bool,
    copy_line_number: Option<u32>,
) -> Element {
    rsx! {
        SplitGutter { side, number: Some(line_number) }
        SplitCodeCell {
            presentation: match side {
                SplitSide::Old => SplitCellPresentation::OldContext,
                SplitSide::New => SplitCellPresentation::NewContext,
            },
            source: CodeLineSource::SplitContext(row),
            artifact_enhancement,
            copy_line_number,
        }
    }
}

#[component]
fn SplitCell(
    row: ReadStore<ViewerSplitRow>,
    line_number: Option<u32>,
    side: SplitSide,
    artifact_enhancement: bool,
    copy_line_number: Option<u32>,
) -> Element {
    let presentation = match side {
        SplitSide::Old => SplitCellPresentation::Removed,
        SplitSide::New => SplitCellPresentation::Added,
    };
    match (side, line_number) {
        (SplitSide::Old, Some(line_number)) => rsx! {
            SplitGutter { side, number: Some(line_number) }
            SplitCodeCell {
                presentation,
                source: CodeLineSource::SplitOld(row),
                artifact_enhancement,
                copy_line_number,
            }
        },
        (SplitSide::New, Some(line_number)) => rsx! {
            SplitGutter { side, number: Some(line_number) }
            SplitCodeCell {
                presentation,
                source: CodeLineSource::SplitNew(row),
                artifact_enhancement,
                copy_line_number,
            }
        },
        (_, None) => rsx! {
            SplitPad { side }
        },
    }
}

#[component]
fn SplitGutter(side: SplitSide, number: Option<u32>) -> Element {
    let line_number = number.map(|value| value.to_string());
    let side_classes = match side {
        SplitSide::Old => "",
        SplitSide::New => "border-l border-line tablet:border-t tablet:border-l-0",
    };
    rsx! {
        span {
            class: "select-none whitespace-nowrap px-2 text-right text-xs text-ink-3 [font-variant-numeric:tabular-nums] mobile:px-1",
            class: "{side_classes}",
            {line_number}
        }
    }
}

#[component]
fn SplitCodeCell(
    presentation: SplitCellPresentation,
    source: CodeLineSource,
    artifact_enhancement: bool,
    copy_line_number: Option<u32>,
) -> Element {
    let marker = Some(match presentation {
        SplitCellPresentation::OldContext | SplitCellPresentation::NewContext => ' ',
        SplitCellPresentation::Removed => '-',
        SplitCellPresentation::Added => '+',
    });
    let changed_text_tone = match presentation {
        SplitCellPresentation::OldContext | SplitCellPresentation::NewContext => {
            ChangedTextTone::None
        }
        SplitCellPresentation::Removed => ChangedTextTone::Removed,
        SplitCellPresentation::Added => ChangedTextTone::Added,
    };
    let presentation_classes = match presentation {
        SplitCellPresentation::OldContext => "bg-transparent",
        SplitCellPresentation::NewContext => "bg-transparent tablet:border-t tablet:border-line",
        SplitCellPresentation::Removed => "bg-del-bg",
        SplitCellPresentation::Added => "bg-add-bg tablet:border-t tablet:border-line",
    };
    let copy_line = copy_line_number.map(|_| "");
    let new_line_number = copy_line_number.map(|number| number.to_string());

    rsx! {
        code {
            class: "{SPLIT_CODE_CELL_CLASSES}",
            class: "{presentation_classes}",
            "data-gtl-copy-line": copy_line,
            "data-gtl-new-line": new_line_number,
            CodeCellContent {
                source,
                marker,
                changed_text_tone,
                artifact_enhancement,
                copy_text: copy_line_number.is_some(),
            }
        }
    }
}

#[component]
fn SplitPad(side: SplitSide) -> Element {
    let side_classes = match side {
        SplitSide::Old => "bg-sunk",
        SplitSide::New => "bg-sunk tablet:border-t tablet:border-line",
    };
    rsx! {
        SplitGutter { side, number: None }
        code { class: "{SPLIT_CODE_CELL_CLASSES}", class: "{side_classes}" }
    }
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::ViewerSplitCell;

    use super::*;
    use crate::test_support::code_line;

    #[component]
    fn SplitBatchFixture(rows: Vec<ViewerSplitRow>, artifact_enhancement: bool) -> Element {
        let rows = use_store(move || rows);
        rsx! {
            SplitRows { rows, artifact_enhancement }
        }
    }

    #[test]
    fn renders_typed_line_numbers_long_lines_and_an_absent_split_cell() {
        let rows = vec![
            ViewerSplitRow::Hunk("@@ -9999,2 +10000 @@".to_owned()),
            ViewerSplitRow::Pair {
                old: Some(ViewerSplitCell {
                    line_number: 9999,
                    code: code_line("abcd", Some(4)),
                }),
                new: Some(ViewerSplitCell {
                    line_number: 10000,
                    code: code_line("abce", Some(4)),
                }),
            },
            ViewerSplitRow::Pair {
                old: Some(ViewerSplitCell {
                    line_number: 10000,
                    code: code_line("tail", Some(4)),
                }),
                new: None,
            },
        ];
        let html = dioxus_ssr::render_element(rsx! {
            SplitBatchFixture { rows, artifact_enhancement: false }
        });
        let absent_gutter = dioxus_ssr::render_element(rsx! {
            SplitGutter { side: SplitSide::New, number: None }
        });

        assert!(html.contains(">9999</span>"));
        assert!(html.contains(">10000</span>"));
        assert_eq!(html.matches("4 chars").count(), 3);
        assert!(html.contains("bg-sunk tablet:border-t tablet:border-line"));
        assert!(absent_gutter.ends_with("></span>"));
        assert_eq!(html.matches(r#"data-gtl-copy-line="""#).count(), 1);
        assert!(html.contains(r#"data-gtl-copy-text="">abce</span>"#));
    }

    #[test]
    fn artifact_rows_copy_only_the_new_side_without_diff_markers() {
        let rows = vec![
            ViewerSplitRow::Hunk("@@ -1,2 +10,3 @@".to_owned()),
            ViewerSplitRow::Context {
                old_line_number: 1,
                new_line_number: 10,
                code: code_line("keep", Some(4)),
            },
            ViewerSplitRow::Pair {
                old: Some(ViewerSplitCell {
                    line_number: 2,
                    code: code_line("oldx", Some(4)),
                }),
                new: Some(ViewerSplitCell {
                    line_number: 11,
                    code: code_line("newx", Some(4)),
                }),
            },
            ViewerSplitRow::Pair {
                old: None,
                new: Some(ViewerSplitCell {
                    line_number: 12,
                    code: code_line("abcdefgh", Some(8)),
                }),
            },
        ];
        let html = dioxus_ssr::render_element(rsx! {
            SplitBatchFixture { rows, artifact_enhancement: true }
        });

        assert_eq!(html.matches(r#"data-gtl-copy-line="""#).count(), 3);
        for line_number in [10, 11, 12] {
            assert!(html.contains(&format!(r#"data-gtl-new-line="{line_number}""#)));
        }
        for text in ["keep", "newx", "abcdefgh"] {
            assert!(html.contains(&format!(r#"data-gtl-copy-text="">{text}</span>"#)));
        }
        assert!(!html.contains(r#"data-gtl-copy-text="">oldx</span>"#));
        assert!(!html.contains(r#"data-gtl-copy-text="">+newx</span>"#));
    }
}
