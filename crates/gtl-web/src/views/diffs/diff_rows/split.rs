use dioxus::prelude::*;

use super::{
    HeaderTone,
    content::{ChangedTextTone, CodeCellContent, CodeLineSource, non_breaking_if_empty},
};
use crate::entities::diffs::{
    ClientDiffFile, ClientDiffFileStoreExt, ClientDiffRowsStoreExt, ViewerSplitRow,
};

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
pub(crate) fn SplitDiffRowBatch(file: ReadStore<ClientDiffFile>, batch_index: usize) -> Element {
    let rows = file.rows().split().index(batch_index);
    rsx! {
        SplitRows { rows, first_row: batch_index * 64 }
    }
}

#[component]
fn SplitRows(rows: ReadStore<Vec<ViewerSplitRow>>, #[props(default)] first_row: usize) -> Element {
    rsx! {
        for (index, row) in rows.iter().enumerate() {
            div {
                key: "{index}",
                "data-row-index": (first_row + index).to_string(),
                SplitDiffRowView { row }
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
fn SplitDiffRowView(row: ReadStore<ViewerSplitRow>) -> Element {
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

                    copy_line_number: None,
                }
                SplitContextCell {
                    row,
                    side: SplitSide::New,
                    line_number: new_line_number,

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

                    copy_line_number: None,
                }
                SplitCell {
                    row,
                    line_number: new_line_number,
                    side: SplitSide::New,

                    copy_line_number: new_line_number,
                }
            }
        },
    }
}

#[component]
fn SplitHeaderRow(tone: HeaderTone, text: String) -> Element {
    let tone = match tone {
        HeaderTone::Meta => "meta",
        HeaderTone::Hunk => "hunk",
    };
    rsx! {
        div {
            class: "diff-row-split-header",
            "data-diff-tone": tone,
            "data-gtl-diff-row": "",
            code { class: "diff-row-header-code diff-row-code", "{text}" }
        }
    }
}

#[component]
fn SplitRowShell(children: Element) -> Element {
    rsx! {
        div { class: "diff-row-split", "data-gtl-diff-row": "", {children} }
    }
}

#[component]
fn SplitContextCell(
    row: ReadStore<ViewerSplitRow>,
    side: SplitSide,
    line_number: u32,
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

            copy_line_number,
        }
    }
}

#[component]
fn SplitCell(
    row: ReadStore<ViewerSplitRow>,
    line_number: Option<u32>,
    side: SplitSide,
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

                copy_line_number,
            }
        },
        (SplitSide::New, Some(line_number)) => rsx! {
            SplitGutter { side, number: Some(line_number) }
            SplitCodeCell {
                presentation,
                source: CodeLineSource::SplitNew(row),

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
    let side = match side {
        SplitSide::Old => "old",
        SplitSide::New => "new",
    };
    rsx! {
        span { class: "diff-row-split-gutter", "data-diff-side": side, {line_number} }
    }
}

#[component]
fn SplitCodeCell(
    presentation: SplitCellPresentation,
    source: CodeLineSource,
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
    let presentation = match presentation {
        SplitCellPresentation::OldContext => "old-context",
        SplitCellPresentation::NewContext => "new-context",
        SplitCellPresentation::Removed => "removed",
        SplitCellPresentation::Added => "added",
    };
    let copy_line = copy_line_number.map(|_| "");
    let new_line_number = copy_line_number.map(|number| number.to_string());

    rsx! {
        code {
            class: "diff-row-split-code diff-row-code",
            "data-diff-cell": presentation,
            "data-gtl-copy-line": copy_line,
            "data-gtl-new-line": new_line_number,
            CodeCellContent {
                source,
                marker,
                changed_text_tone,

                copy_text: copy_line_number.is_some(),
            }
        }
    }
}

#[component]
fn SplitPad(side: SplitSide) -> Element {
    let cell = match side {
        SplitSide::Old => "pad-old",
        SplitSide::New => "pad-new",
    };
    rsx! {
        SplitGutter { side, number: None }
        code {
            class: "diff-row-split-code diff-row-code",
            "data-diff-cell": cell,
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::ViewerSplitCell;

    use super::*;
    use crate::test_support::code_line;

    #[component]
    fn SplitBatchFixture(rows: Vec<ViewerSplitRow>) -> Element {
        let rows = use_store(move || rows);
        rsx! {
            SplitRows { rows }
        }
    }

    #[test]
    fn renders_typed_line_numbers_truncated_lines_and_an_absent_split_cell() {
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
            SplitBatchFixture { rows }
        });
        let absent_gutter = dioxus_ssr::render_element(rsx! {
            SplitGutter { side: SplitSide::New, number: None }
        });

        assert!(html.contains(">9999</span>"));
        assert!(html.contains(">10000</span>"));
        assert_eq!(html.matches("(+4 characters omitted)").count(), 3);
        assert!(html.contains(r#"data-diff-cell="pad-new""#));
        assert!(absent_gutter.ends_with("></span>"));
        assert_eq!(html.matches(r#"data-gtl-copy-line="""#).count(), 1);
        assert!(html.contains(r#"class="diff-truncated-text">abce</span>"#));
    }
}
