use std::sync::Arc;

use dioxus::prelude::*;
use gtl_parser::{CharacterCount, DiffRow, DiffRowKind, SemanticTextSpan, SourceLineNumber};

use super::{
    HeaderTone,
    content::{ChangedTextTone, CodeCellContent, non_breaking_if_empty},
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
pub(crate) fn UnifiedDiffRowBatch(rows: Arc<Vec<DiffRow>>) -> Element {
    rsx! {
        for (index, row) in rows.iter().cloned().enumerate() {
            UnifiedDiffRow { key: "{index}", row }
        }
    }
}

#[component]
fn UnifiedDiffRow(row: DiffRow) -> Element {
    match row.kind() {
        DiffRowKind::Meta => rsx! {
            UnifiedHeaderRow {
                tone: HeaderTone::Meta,
                text: non_breaking_if_empty(row.text()),
            }
        },
        DiffRowKind::Hunk => rsx! {
            UnifiedHeaderRow {
                tone: HeaderTone::Hunk,
                text: non_breaking_if_empty(row.text()),
            }
        },
        DiffRowKind::Context => rsx! {
            UnifiedSourceRow { row, tone: UnifiedSourceTone::Context }
        },
        DiffRowKind::Added => rsx! {
            UnifiedSourceRow { row, tone: UnifiedSourceTone::Added }
        },
        DiffRowKind::Removed => rsx! {
            UnifiedSourceRow { row, tone: UnifiedSourceTone::Removed }
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
fn UnifiedSourceRow(row: DiffRow, tone: UnifiedSourceTone) -> Element {
    let (old_gutter_tone, new_gutter_tone) = match tone {
        UnifiedSourceTone::Context => (UnifiedGutterTone::Hidden, UnifiedGutterTone::Neutral),
        UnifiedSourceTone::Added => (UnifiedGutterTone::Hidden, UnifiedGutterTone::Added),
        UnifiedSourceTone::Removed => (UnifiedGutterTone::Removed, UnifiedGutterTone::Hidden),
    };

    rsx! {
        UnifiedSourceRowShell { tone,
            UnifiedGutter { number: row.old_line_number(), tone: old_gutter_tone }
            UnifiedGutter { number: row.new_line_number(), tone: new_gutter_tone }
            UnifiedCodeCell {
                text: row.body().to_owned(),
                semantic_spans: row.semantic_spans().to_vec(),
                long_line_character_count: row.long_line_character_count(),
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
fn UnifiedSourceRowShell(tone: UnifiedSourceTone, children: Element) -> Element {
    let tone_classes = match tone {
        UnifiedSourceTone::Context => "",
        UnifiedSourceTone::Added => "bg-[color-mix(in_srgb,var(--add-bg)_50%,transparent)]",
        UnifiedSourceTone::Removed => "bg-[color-mix(in_srgb,var(--del-bg)_50%,transparent)]",
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
fn UnifiedGutter(number: Option<SourceLineNumber>, tone: UnifiedGutterTone) -> Element {
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
    text: String,
    semantic_spans: Vec<SemanticTextSpan>,
    long_line_character_count: Option<CharacterCount>,
) -> Element {
    rsx! {
        code { class: "col-start-2 row-start-1 min-w-0 border-0 bg-transparent py-0 pr-1 pl-3 text-sm text-code whitespace-pre-wrap [overflow-wrap:anywhere] print:text-[#111]",
            CodeCellContent {
                text,
                semantic_spans,
                changed_text_tone: ChangedTextTone::None,
                long_line_character_count,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_parser::{CharacterCount, DiffParser, ParseOptions};

    use super::*;

    #[test]
    fn renders_typed_line_numbers_absent_gutters_and_long_line_counts() {
        let parsed = DiffParser::with_options(ParseOptions::new(CharacterCount::new(3))).parse(&[
            "@@ -9999 +10000 @@".to_owned(),
            "-abcd".to_owned(),
            "+abce".to_owned(),
        ]);
        let html = dioxus_ssr::render_element(rsx! {
            UnifiedDiffRowBatch { rows: Arc::new(parsed.into_rows()) }
        });

        assert!(html.contains(">9999</span>"));
        assert!(html.contains(">10000</span>"));
        assert_eq!(html.matches("class=\"hidden").count(), 4);
        assert_eq!(html.matches("4 chars").count(), 2);
    }
}
