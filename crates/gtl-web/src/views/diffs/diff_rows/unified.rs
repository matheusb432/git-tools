use dioxus::prelude::*;

use super::{
    HeaderTone,
    content::{ChangedTextTone, CodeCellContent, CodeLineSource, non_breaking_if_empty},
};
use crate::entities::diffs::{
    ClientDiffFileStoreExt, ClientDiffRowsStoreExt, ClientDiffWorkspace,
    ClientDiffWorkspaceStoreExt, ViewerUnifiedRow,
};

const HEADER_CODE_CLASSES: &str = "min-w-0 border-0 bg-transparent px-3 text-sm text-ink-3 whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 print:text-[#111]";
const UNIFIED_GUTTER_CLASSES: &str = "col-start-1 row-start-1 select-none whitespace-nowrap px-0 text-center text-sm [font-variant-numeric:tabular-nums]";
const UNIFIED_ROW_SHELL_CLASSES: &str = "relative grid grid-cols-[max(28px,var(--unified-line-number-width,28px))_minmax(0,1fr)] items-start whitespace-normal";

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
pub(crate) fn UnifiedDiffRowBatch(
    file_index: usize,
    batch_index: usize,
    artifact_enhancement: bool,
) -> Element {
    let workspace = use_context::<Store<ClientDiffWorkspace>>();
    let Some(file) = workspace.files().get(file_index) else {
        return rsx! {};
    };
    let Some(rows) = file.rows().unified().get(batch_index) else {
        return rsx! {};
    };
    let rows: ReadStore<Vec<ViewerUnifiedRow>> = rows.into();
    rsx! {
        UnifiedRows { rows, artifact_enhancement }
    }
}

#[component]
fn UnifiedRows(rows: ReadStore<Vec<ViewerUnifiedRow>>, artifact_enhancement: bool) -> Element {
    rsx! {
        for (index, row) in rows.iter().enumerate() {
            UnifiedDiffRow { key: "{index}", row, artifact_enhancement }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UnifiedRowPresentation {
    Header { tone: HeaderTone, text: String },
    Source(UnifiedSourceTone),
}

#[component]
fn UnifiedDiffRow(row: ReadStore<ViewerUnifiedRow>, artifact_enhancement: bool) -> Element {
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
            UnifiedSourceRow { row, tone, artifact_enhancement }
        },
    }
}

#[component]
fn UnifiedHeaderRow(tone: HeaderTone, text: String) -> Element {
    rsx! {
        UnifiedHeaderRowShell { tone,
            UnifiedGutter { number: None, tone: UnifiedGutterTone::Hidden }
            UnifiedGutter { number: None, tone: UnifiedGutterTone::Hidden }
            UnifiedHeaderCode { tone, text }
        }
    }
}

#[component]
fn UnifiedSourceRow(
    row: ReadStore<ViewerUnifiedRow>,
    tone: UnifiedSourceTone,
    artifact_enhancement: bool,
) -> Element {
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
            UnifiedCodeCell {
                row,
                artifact_enhancement,
                copy_text: copy_line_number.is_some(),
            }
        }
    }
}

#[component]
fn UnifiedHeaderRowShell(tone: HeaderTone, children: Element) -> Element {
    let tone_classes = match tone {
        HeaderTone::Meta => "opacity-60",
        HeaderTone::Hunk => "bg-sunk",
    };
    rsx! {
        div {
            class: "{UNIFIED_ROW_SHELL_CLASSES}",
            class: "{tone_classes}",
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
    let tone_classes = match tone {
        UnifiedSourceTone::Context => "",
        UnifiedSourceTone::Added => "bg-[color-mix(in_srgb,var(--add-bg)_50%,transparent)]",
        UnifiedSourceTone::Removed => "bg-[color-mix(in_srgb,var(--del-bg)_50%,transparent)]",
    };
    let copy_line = copy_line_number.map(|_| "");
    let new_line_number = copy_line_number.map(|number| number.to_string());
    rsx! {
        div {
            class: "{UNIFIED_ROW_SHELL_CLASSES}",
            class: "{tone_classes}",
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
            UNIFIED_GUTTER_CLASSES
        }
    };
    let tone_classes = match tone {
        UnifiedGutterTone::Hidden => "",
        UnifiedGutterTone::Neutral => "bg-transparent text-ink-3",
        UnifiedGutterTone::Added => "bg-add-gut text-add",
        UnifiedGutterTone::Removed => "bg-del-gut text-del",
    };
    rsx! {
        span { class: "{gutter_classes}", class: "{tone_classes}", {line_number} }
    }
}

#[component]
fn UnifiedHeaderCode(tone: HeaderTone, text: String) -> Element {
    let tone_classes = match tone {
        HeaderTone::Meta => "",
        HeaderTone::Hunk => "font-semibold",
    };
    rsx! {
        code {
            class: "{HEADER_CODE_CLASSES} col-[1/-1]",
            class: "{tone_classes}",
            "{text}"
        }
    }
}

#[component]
fn UnifiedCodeCell(
    row: ReadStore<ViewerUnifiedRow>,
    artifact_enhancement: bool,
    copy_text: bool,
) -> Element {
    rsx! {
        code { class: "col-start-2 row-start-1 min-w-0 border-0 bg-transparent py-0 pr-1 pl-3 text-sm text-code whitespace-pre-wrap [overflow-wrap:anywhere] print:text-[#111]",
            CodeCellContent {
                source: CodeLineSource::Unified(row),
                marker: None,
                changed_text_tone: ChangedTextTone::None,
                artifact_enhancement,
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
    fn UnifiedBatchFixture(rows: Vec<ViewerUnifiedRow>, artifact_enhancement: bool) -> Element {
        let rows = use_store(move || rows);
        rsx! {
            UnifiedRows { rows, artifact_enhancement }
        }
    }

    #[test]
    fn renders_typed_line_numbers_absent_gutters_and_long_line_counts() {
        let rows = vec![
            ViewerUnifiedRow::Hunk("@@ -9999 +10000 @@".to_owned()),
            ViewerUnifiedRow::Removed(unified_source_row("abcd", Some(9999), None, Some(4))),
            ViewerUnifiedRow::Added(unified_source_row("abce", None, Some(10000), Some(4))),
        ];
        let html = dioxus_ssr::render_element(rsx! {
            UnifiedBatchFixture { rows, artifact_enhancement: false }
        });

        assert!(html.contains(">9999</span>"));
        assert!(html.contains(">10000</span>"));
        assert_eq!(html.matches("class=\"hidden").count(), 4);
        assert_eq!(html.matches("4 chars").count(), 2);
        assert_eq!(html.matches(r#"data-gtl-copy-line="""#).count(), 1);
        assert!(html.contains(r#"data-gtl-copy-text="">abce</span>"#));
    }

    #[test]
    fn artifact_rows_designate_copyable_new_source_without_markers() {
        let rows = vec![
            ViewerUnifiedRow::Hunk("@@ -1,2 +10,3 @@".to_owned()),
            ViewerUnifiedRow::Context(unified_source_row("keep", Some(1), Some(10), Some(4))),
            ViewerUnifiedRow::Removed(unified_source_row("oldx", Some(2), None, Some(4))),
            ViewerUnifiedRow::Added(unified_source_row("newx", None, Some(11), Some(4))),
            ViewerUnifiedRow::Added(unified_source_row("abcdefgh", None, Some(12), Some(8))),
        ];
        let html = dioxus_ssr::render_element(rsx! {
            UnifiedBatchFixture { rows, artifact_enhancement: true }
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
        assert!(html.contains(r#"data-gtl-long-line="""#));
        assert!(html.contains(r#"data-gtl-expanded="false""#));
        assert!(html.contains(r#"data-gtl-action="toggle-long-line""#));
        assert!(html.contains(r#"data-gtl-long-line-text="""#));
        assert!(html.contains(r#"data-gtl-expanded-classes="overflow-x-auto text-clip""#));
        assert!(html.contains(r#"data-gtl-collapsed-classes="overflow-hidden text-ellipsis""#));
    }
}
