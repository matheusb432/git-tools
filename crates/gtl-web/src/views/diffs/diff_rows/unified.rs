use dioxus::prelude::*;

use super::{
    HeaderTone,
    content::{ChangedTextTone, CodeCellContent, CodeLineSource, non_breaking_if_empty},
};
use crate::entities::diffs::{
    ClientDiffFile, ClientDiffFileStoreExt, ClientDiffRowsStoreExt, ViewerUnifiedRow,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnifiedSourceTone {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnifiedGutterTone {
    Hidden,
    Neutral,
    Added,
    Removed,
}

#[component]
pub(crate) fn UnifiedDiffRowBatch(file: ReadStore<ClientDiffFile>, batch_index: usize) -> Element {
    let rows = file.rows().unified().index(batch_index);
    rsx! {
        UnifiedRows { rows, first_row: batch_index * 64 }
    }
}

#[component]
fn UnifiedRows(
    rows: ReadStore<Vec<ViewerUnifiedRow>>,

    #[props(default)] first_row: usize,
) -> Element {
    rsx! {
        for (index, row) in rows.iter().enumerate() {
            div {
                key: "{index}",
                "data-row-index": (first_row + index).to_string(),
                UnifiedDiffRow { row }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UnifiedRowPresentation {
    Header { tone: HeaderTone, text: String },
    Source(UnifiedSourceTone),
}

#[component]
fn UnifiedDiffRow(row: ReadStore<ViewerUnifiedRow>) -> Element {
    let presentation = {
        let row = row.read();
        match &*row {
            ViewerUnifiedRow::Meta(text) => UnifiedRowPresentation::Header {
                tone: HeaderTone::Meta,
                text: non_breaking_if_empty(text),
            },
            ViewerUnifiedRow::Hunk(text) => UnifiedRowPresentation::Header {
                tone: HeaderTone::Hunk,
                text: non_breaking_if_empty(text),
            },
            ViewerUnifiedRow::Context(_) => {
                UnifiedRowPresentation::Source(UnifiedSourceTone::Context)
            }
            ViewerUnifiedRow::Added(_) => UnifiedRowPresentation::Source(UnifiedSourceTone::Added),
            ViewerUnifiedRow::Removed(_) => {
                UnifiedRowPresentation::Source(UnifiedSourceTone::Removed)
            }
        }
    };

    match presentation {
        UnifiedRowPresentation::Header { tone, text } => rsx! {
            UnifiedHeaderRow { tone, text }
        },
        UnifiedRowPresentation::Source(tone) => rsx! {
            UnifiedSourceRow { row, tone }
        },
    }
}

#[component]
fn UnifiedHeaderRow(tone: HeaderTone, text: String) -> Element {
    rsx! {
        UnifiedHeaderRowShell { tone,
            UnifiedGutter { number: None, tone: UnifiedGutterTone::Hidden }
            UnifiedGutter { number: None, tone: UnifiedGutterTone::Hidden }
            UnifiedHeaderCode { text }
        }
    }
}

#[component]
fn UnifiedSourceRow(row: ReadStore<ViewerUnifiedRow>, tone: UnifiedSourceTone) -> Element {
    let line_numbers = {
        let row = row.read();
        match &*row {
            ViewerUnifiedRow::Context(source)
            | ViewerUnifiedRow::Added(source)
            | ViewerUnifiedRow::Removed(source) => {
                Some((source.old_line_number, source.new_line_number))
            }
            ViewerUnifiedRow::Meta(_) | ViewerUnifiedRow::Hunk(_) => None,
        }
    };
    let Some((old_line_number, new_line_number)) = line_numbers else {
        return rsx! {};
    };
    let (old_gutter_tone, new_gutter_tone) = match tone {
        UnifiedSourceTone::Context => (UnifiedGutterTone::Hidden, UnifiedGutterTone::Neutral),
        UnifiedSourceTone::Added => (UnifiedGutterTone::Hidden, UnifiedGutterTone::Added),
        UnifiedSourceTone::Removed => (UnifiedGutterTone::Removed, UnifiedGutterTone::Hidden),
    };
    let copy_line_number = if tone == UnifiedSourceTone::Removed {
        None
    } else {
        new_line_number
    };

    rsx! {
        UnifiedSourceRowShell { tone, copy_line_number,
            UnifiedGutter { number: old_line_number, tone: old_gutter_tone }
            UnifiedGutter { number: new_line_number, tone: new_gutter_tone }
            UnifiedCodeCell { row, copy_text: copy_line_number.is_some() }
        }
    }
}

#[component]
fn UnifiedHeaderRowShell(tone: HeaderTone, children: Element) -> Element {
    let tone = match tone {
        HeaderTone::Meta => "meta",
        HeaderTone::Hunk => "hunk",
    };
    rsx! {
        div {
            class: "diff-row-unified",
            "data-diff-tone": tone,
            "data-gtl-diff-row": "",
            {children}
        }
    }
}

#[component]
fn UnifiedSourceRowShell(
    tone: UnifiedSourceTone,
    copy_line_number: Option<u32>,
    children: Element,
) -> Element {
    let tone = match tone {
        UnifiedSourceTone::Context => "context",
        UnifiedSourceTone::Added => "added",
        UnifiedSourceTone::Removed => "removed",
    };
    let copy_line = copy_line_number.map(|_| "");
    let new_line_number = copy_line_number.map(|number| number.to_string());
    rsx! {
        div {
            class: "diff-row-unified",
            "data-diff-tone": tone,
            "data-gtl-diff-row": "",
            "data-gtl-copy-line": copy_line,
            "data-gtl-new-line": new_line_number,
            {children}
        }
    }
}

#[component]
fn UnifiedGutter(number: Option<u32>, tone: UnifiedGutterTone) -> Element {
    let line_number = number.map(|value| value.to_string());
    let gutter_classes = match tone {
        UnifiedGutterTone::Hidden => "hidden",
        UnifiedGutterTone::Neutral | UnifiedGutterTone::Added | UnifiedGutterTone::Removed => {
            "diff-row-unified-gutter"
        }
    };
    let tone = match tone {
        UnifiedGutterTone::Hidden => None,
        UnifiedGutterTone::Neutral => Some("neutral"),
        UnifiedGutterTone::Added => Some("added"),
        UnifiedGutterTone::Removed => Some("removed"),
    };
    rsx! {
        span { class: "{gutter_classes}", "data-diff-tone": tone, {line_number} }
    }
}

#[component]
fn UnifiedHeaderCode(text: String) -> Element {
    rsx! {
        code { class: "diff-row-header-code diff-row-code col-[1/-1]", "{text}" }
    }
}

#[component]
fn UnifiedCodeCell(row: ReadStore<ViewerUnifiedRow>, copy_text: bool) -> Element {
    rsx! {
        code { class: "diff-row-unified-code min-w-0 py-0 pr-1 pl-3 text-sm diff-row-code",
            CodeCellContent {
                source: CodeLineSource::Unified(row),
                marker: None,
                changed_text_tone: ChangedTextTone::None,

                copy_text,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unified_source_row;

    #[component]
    fn UnifiedBatchFixture(rows: Vec<ViewerUnifiedRow>) -> Element {
        let rows = use_store(move || rows);
        rsx! {
            UnifiedRows { rows }
        }
    }

    #[test]
    fn renders_typed_line_numbers_absent_gutters_and_omission_counts() {
        let rows = vec![
            ViewerUnifiedRow::Hunk("@@ -9999 +10000 @@".to_owned()),
            ViewerUnifiedRow::Removed(unified_source_row("abcd", Some(9999), None, Some(4))),
            ViewerUnifiedRow::Added(unified_source_row("abce", None, Some(10000), Some(4))),
        ];
        let html = dioxus_ssr::render_element(rsx! {
            UnifiedBatchFixture { rows }
        });

        assert!(html.contains(">9999</span>"));
        assert!(html.contains(">10000</span>"));
        assert_eq!(html.matches("class=\"hidden").count(), 4);
        assert_eq!(html.matches("(+4 characters omitted)").count(), 2);
        assert_eq!(html.matches(r#"data-gtl-copy-line="""#).count(), 1);
        assert!(html.contains(r#"class="diff-truncated-text">abce</span>"#));
    }
}
