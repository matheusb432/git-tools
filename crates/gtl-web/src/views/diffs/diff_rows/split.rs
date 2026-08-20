use std::sync::Arc;

use dioxus::prelude::*;
use gtl_parser::{
    CharacterCount, SemanticTextSpan, SourceLineNumber, SplitDiffCell, SplitDiffRow, diff_line_body,
};

use super::{
    HeaderTone,
    content::{ChangedTextTone, LongLine, SemanticText, non_breaking_if_empty},
};

const HEADER_CODE_CLASSES: &str = "min-w-0 border-0 bg-transparent px-3 text-sm text-ink-3 whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 print:text-[#111]";
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
    rows: Arc<Vec<SplitDiffRow>>,
    artifact_enhancement: bool,
) -> Element {
    rsx! {
        for (index, row) in rows.iter().cloned().enumerate() {
            SplitDiffRowView { key: "{index}", row, artifact_enhancement }
        }
    }
}

#[component]
fn SplitDiffRowView(row: SplitDiffRow, artifact_enhancement: bool) -> Element {
    match row {
        SplitDiffRow::Meta { text } => rsx! {
            SplitHeaderRow { tone: HeaderTone::Meta, text: non_breaking_if_empty(&text) }
        },
        SplitDiffRow::Hunk { text } => rsx! {
            SplitHeaderRow { tone: HeaderTone::Hunk, text }
        },
        SplitDiffRow::Context {
            old_line_number,
            new_line_number,
            text,
            syntax_tokens: _,
            semantic_spans,
            long_line_character_count,
        } => rsx! {
            SplitRowShell { copy_line_number: artifact_enhancement.then_some(new_line_number),
                SplitContextCell {
                    side: SplitSide::Old,
                    line_number: old_line_number,
                    text: text.clone(),
                    semantic_spans: semantic_spans.clone(),
                    long_line_character_count,
                    artifact_enhancement,
                    copy_text: false,
                }
                SplitContextCell {
                    side: SplitSide::New,
                    line_number: new_line_number,
                    text,
                    semantic_spans,
                    long_line_character_count,
                    artifact_enhancement,
                    copy_text: artifact_enhancement,
                }
            }
        },
        SplitDiffRow::Pair { old, new } => {
            let copy_line_number = if artifact_enhancement {
                new.as_ref().map(SplitDiffCell::line_number)
            } else {
                None
            };
            rsx! {
                SplitRowShell { copy_line_number,
                    SplitCell {
                        cell: old,
                        side: SplitSide::Old,
                        artifact_enhancement,
                        copy_text: false,
                    }
                    SplitCell {
                        cell: new,
                        side: SplitSide::New,
                        artifact_enhancement,
                        copy_text: copy_line_number.is_some(),
                    }
                }
            }
        }
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
fn SplitRowShell(copy_line_number: Option<SourceLineNumber>, children: Element) -> Element {
    let copy_line = copy_line_number.map(|_| "");
    let new_line_number = copy_line_number.map(|number| number.to_string());
    rsx! {
        div {
            class: "grid grid-cols-[44px_minmax(0,1fr)_44px_minmax(0,1fr)] items-stretch whitespace-normal tablet:grid-cols-[44px_minmax(0,1fr)] mobile:grid-cols-[30px_minmax(0,1fr)]",
            "data-gtl-diff-row": "",
            "data-gtl-copy-line": copy_line,
            "data-gtl-new-line": new_line_number,
            {children}
        }
    }
}

#[component]
fn SplitContextCell(
    side: SplitSide,
    line_number: SourceLineNumber,
    text: String,
    semantic_spans: Vec<SemanticTextSpan>,
    long_line_character_count: Option<CharacterCount>,
    artifact_enhancement: bool,
    copy_text: bool,
) -> Element {
    rsx! {
        SplitGutter { side, number: Some(line_number) }
        SplitCodeCell {
            presentation: match side {
                SplitSide::Old => SplitCellPresentation::OldContext,
                SplitSide::New => SplitCellPresentation::NewContext,
            },
            text,
            semantic_spans,
            long_line_character_count,
            artifact_enhancement,
            copy_text,
        }
    }
}

#[component]
fn SplitCell(
    cell: Option<SplitDiffCell>,
    side: SplitSide,
    artifact_enhancement: bool,
    copy_text: bool,
) -> Element {
    let presentation = match side {
        SplitSide::Old => SplitCellPresentation::Removed,
        SplitSide::New => SplitCellPresentation::Added,
    };
    match cell {
        Some(cell) => rsx! {
            SplitGutter { side, number: Some(cell.line_number()) }
            SplitCodeCell {
                presentation,
                text: cell.text().to_owned(),
                semantic_spans: cell.semantic_spans().to_vec(),
                long_line_character_count: cell.long_line_character_count(),
                artifact_enhancement,
                copy_text,
            }
        },
        None => rsx! {
            SplitPad { side }
        },
    }
}

#[component]
fn SplitGutter(side: SplitSide, number: Option<SourceLineNumber>) -> Element {
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
    text: String,
    semantic_spans: Vec<SemanticTextSpan>,
    long_line_character_count: Option<CharacterCount>,
    artifact_enhancement: bool,
    copy_text: bool,
) -> Element {
    let marker = text.chars().next();
    let body = diff_line_body(&text).to_owned();
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

    rsx! {
        code {
            class: "{SPLIT_CODE_CELL_CLASSES}",
            class: "{presentation_classes}",
            SplitCodeContent {
                marker,
                body,
                semantic_spans,
                changed_text_tone,
                long_line_character_count,
                artifact_enhancement,
                copy_text,
            }
        }
    }
}

#[component]
fn SplitCodeContent(
    marker: Option<char>,
    body: String,
    semantic_spans: Vec<SemanticTextSpan>,
    changed_text_tone: ChangedTextTone,
    long_line_character_count: Option<CharacterCount>,
    artifact_enhancement: bool,
    copy_text: bool,
) -> Element {
    if let Some(character_count) = long_line_character_count {
        return rsx! {
            LongLine {
                text: body,
                marker,
                character_count,
                artifact_enhancement,
                copy_text,
            }
        };
    }

    rsx! {
        SplitMarker { marker }
        SemanticText {
            text: body,
            semantic_spans,
            changed_text_tone,
            copy_text,
        }
    }
}

#[component]
fn SplitMarker(marker: Option<char>) -> Element {
    let marker = marker.map(|value| value.to_string());
    rsx! {
        span { {marker} }
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
    use gtl_parser::{CharacterCount, DiffParser, ParseOptions};

    use super::*;

    #[test]
    fn renders_typed_line_numbers_long_lines_and_an_absent_split_cell() {
        let parsed = DiffParser::with_options(ParseOptions::new(CharacterCount::new(3))).parse(&[
            "@@ -9999,2 +10000 @@".to_owned(),
            "-abcd".to_owned(),
            "-tail".to_owned(),
            "+abce".to_owned(),
        ]);
        let html = dioxus_ssr::render_element(rsx! {
            SplitDiffRowBatch {
                rows: Arc::new(parsed.split_rows()),
                artifact_enhancement: false,
            }
        });
        let absent_gutter = dioxus_ssr::render_element(rsx! {
            SplitGutter { side: SplitSide::New, number: None }
        });

        assert!(html.contains(">9999</span>"));
        assert!(html.contains(">10000</span>"));
        assert_eq!(html.matches("4 chars").count(), 3);
        assert!(html.contains("bg-sunk tablet:border-t tablet:border-line"));
        assert!(absent_gutter.ends_with("></span>"));
    }

    #[test]
    fn artifact_rows_copy_only_the_new_side_without_diff_markers() {
        let parsed = DiffParser::with_options(ParseOptions::new(CharacterCount::new(3))).parse(&[
            "@@ -1,2 +10,3 @@".to_owned(),
            " keep".to_owned(),
            "-oldx".to_owned(),
            "+newx".to_owned(),
            "+abcdefgh".to_owned(),
        ]);
        let html = dioxus_ssr::render_element(rsx! {
            SplitDiffRowBatch {
                rows: Arc::new(parsed.split_rows()),
                artifact_enhancement: true,
            }
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
