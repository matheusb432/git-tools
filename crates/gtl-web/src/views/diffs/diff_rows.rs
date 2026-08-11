use std::sync::Arc;

use dioxus::prelude::*;
use gtl_parser::{
    DiffRow, DiffRowKind, SemanticTextChange, SemanticTextSpan, SplitDiffCell, SplitDiffRow,
    SyntaxTokenClass, diff_line_body,
};

use crate::shared::ui::{Button, ButtonSize, ButtonVariant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderTone {
    Meta,
    Hunk,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangedTextTone {
    None,
    Removed,
    Added,
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
    match tone {
        HeaderTone::Meta => rsx! {
            div { class: "relative grid grid-cols-[max(28px,var(--unified-line-number-width,28px))_minmax(0,1fr)] items-start whitespace-normal opacity-60",
                {children}
            }
        },
        HeaderTone::Hunk => rsx! {
            div { class: "relative grid grid-cols-[max(28px,var(--unified-line-number-width,28px))_minmax(0,1fr)] items-start bg-sunk whitespace-normal",
                {children}
            }
        },
    }
}

#[component]
fn UnifiedSourceRowShell(tone: UnifiedSourceTone, children: Element) -> Element {
    match tone {
        UnifiedSourceTone::Context => rsx! {
            div { class: "relative grid grid-cols-[max(28px,var(--unified-line-number-width,28px))_minmax(0,1fr)] items-start whitespace-normal",
                {children}
            }
        },
        UnifiedSourceTone::Added => rsx! {
            div { class: "relative grid grid-cols-[max(28px,var(--unified-line-number-width,28px))_minmax(0,1fr)] items-start bg-[color-mix(in_srgb,var(--add-bg)_50%,transparent)] whitespace-normal",
                {children}
            }
        },
        UnifiedSourceTone::Removed => rsx! {
            div { class: "relative grid grid-cols-[max(28px,var(--unified-line-number-width,28px))_minmax(0,1fr)] items-start bg-[color-mix(in_srgb,var(--del-bg)_50%,transparent)] whitespace-normal",
                {children}
            }
        },
    }
}

#[component]
fn UnifiedGutter(number: Option<u32>, tone: UnifiedGutterTone) -> Element {
    let line_number = number.map(|value| value.to_string());
    match tone {
        UnifiedGutterTone::Hidden => rsx! {
            span { class: "hidden" }
        },
        UnifiedGutterTone::Neutral => rsx! {
            span { class: "col-start-1 row-start-1 select-none whitespace-nowrap bg-transparent px-0 text-center text-[14px] text-ink-3 [font-variant-numeric:tabular-nums] mobile:text-[13px]",
                {line_number}
            }
        },
        UnifiedGutterTone::Added => rsx! {
            span { class: "col-start-1 row-start-1 select-none whitespace-nowrap bg-add-gut px-0 text-center text-[14px] text-add [font-variant-numeric:tabular-nums] mobile:text-[13px]",
                {line_number}
            }
        },
        UnifiedGutterTone::Removed => rsx! {
            span { class: "col-start-1 row-start-1 select-none whitespace-nowrap bg-del-gut px-0 text-center text-[14px] text-del [font-variant-numeric:tabular-nums] mobile:text-[13px]",
                {line_number}
            }
        },
    }
}

#[component]
fn UnifiedHeaderCode(tone: HeaderTone, text: String) -> Element {
    match tone {
        HeaderTone::Hunk => rsx! {
            code { class: "col-[1/-1] min-w-0 border-0 bg-transparent px-3 text-[14px] font-semibold text-ink-3 whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 mobile:text-[13px] print:text-[#111]",
                "{text}"
            }
        },
        HeaderTone::Meta => rsx! {
            code { class: "col-[1/-1] min-w-0 border-0 bg-transparent px-3 text-[14px] text-ink-3 whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 mobile:text-[13px] print:text-[#111]",
                "{text}"
            }
        },
    }
}

#[component]
fn UnifiedCodeCell(
    text: String,
    semantic_spans: Vec<SemanticTextSpan>,
    long_line_character_count: Option<usize>,
) -> Element {
    rsx! {
        code { class: "col-start-2 row-start-1 min-w-0 border-0 bg-transparent py-0 pr-1 pl-3 text-[14px] text-code whitespace-pre-wrap [overflow-wrap:anywhere] mobile:text-[13px] print:text-[#111]",
            CodeCellContent {
                text,
                semantic_spans,
                changed_text_tone: ChangedTextTone::None,
                long_line_character_count,
            }
        }
    }
}

#[component]
pub(crate) fn SplitDiffRowBatch(rows: Arc<Vec<SplitDiffRow>>) -> Element {
    rsx! {
        for (index, row) in rows.iter().cloned().enumerate() {
            SplitDiffRowView { key: "{index}", row }
        }
    }
}

#[component]
fn SplitDiffRowView(row: SplitDiffRow) -> Element {
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
            SplitRowShell {
                SplitContextCell {
                    side: SplitSide::Old,
                    line_number: old_line_number,
                    text: text.clone(),
                    semantic_spans: semantic_spans.clone(),
                    long_line_character_count,
                }
                SplitContextCell {
                    side: SplitSide::New,
                    line_number: new_line_number,
                    text,
                    semantic_spans,
                    long_line_character_count,
                }
            }
        },
        SplitDiffRow::Pair { old, new } => rsx! {
            SplitRowShell {
                SplitCell { cell: old, side: SplitSide::Old }
                SplitCell { cell: new, side: SplitSide::New }
            }
        },
    }
}

#[component]
fn SplitHeaderRow(tone: HeaderTone, text: String) -> Element {
    match tone {
        HeaderTone::Hunk => rsx! {
            div { class: "grid grid-cols-[minmax(0,1fr)] items-stretch bg-sunk whitespace-normal",
                code { class: "min-w-0 border-0 bg-transparent px-3 text-[14px] font-semibold text-ink-3 whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 mobile:text-[13px] print:text-[#111]",
                    "{text}"
                }
            }
        },
        HeaderTone::Meta => rsx! {
            div { class: "grid grid-cols-[minmax(0,1fr)] items-stretch whitespace-normal opacity-60",
                code { class: "min-w-0 border-0 bg-transparent px-3 text-[14px] text-ink-3 whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 mobile:text-[13px] print:text-[#111]",
                    "{text}"
                }
            }
        },
    }
}

#[component]
fn SplitRowShell(children: Element) -> Element {
    rsx! {
        div { class: "grid grid-cols-[44px_minmax(0,1fr)_44px_minmax(0,1fr)] items-stretch whitespace-normal tablet:grid-cols-[44px_minmax(0,1fr)] mobile:grid-cols-[30px_minmax(0,1fr)]",
            {children}
        }
    }
}

#[component]
fn SplitContextCell(
    side: SplitSide,
    line_number: u32,
    text: String,
    semantic_spans: Vec<SemanticTextSpan>,
    long_line_character_count: Option<usize>,
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
        }
    }
}

#[component]
fn SplitCell(cell: Option<SplitDiffCell>, side: SplitSide) -> Element {
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
            }
        },
        None => rsx! {
            SplitPad { side }
        },
    }
}

#[component]
fn SplitGutter(side: SplitSide, number: Option<u32>) -> Element {
    let line_number = number.map(|value| value.to_string());
    match side {
        SplitSide::Old => rsx! {
            span { class: "select-none whitespace-nowrap px-2 text-right text-[12px] text-ink-3 [font-variant-numeric:tabular-nums] mobile:px-1 mobile:text-[10px]",
                {line_number}
            }
        },
        SplitSide::New => rsx! {
            span { class: "select-none whitespace-nowrap border-l border-line px-2 text-right text-[12px] text-ink-3 [font-variant-numeric:tabular-nums] tablet:border-t tablet:border-l-0 mobile:px-1 mobile:text-[10px]",
                {line_number}
            }
        },
    }
}

#[component]
fn SplitCodeCell(
    presentation: SplitCellPresentation,
    text: String,
    semantic_spans: Vec<SemanticTextSpan>,
    long_line_character_count: Option<usize>,
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

    match presentation {
        SplitCellPresentation::OldContext => rsx! {
            code { class: "min-w-0 border-0 bg-transparent px-3 text-[14px] text-code whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 mobile:text-[13px] print:text-[#111]",
                SplitCodeContent {
                    marker,
                    body,
                    long_text: text,
                    semantic_spans,
                    changed_text_tone,
                    long_line_character_count,
                }
            }
        },
        SplitCellPresentation::NewContext => rsx! {
            code { class: "min-w-0 border-0 bg-transparent px-3 text-[14px] text-code whitespace-pre-wrap [overflow-wrap:anywhere] tablet:border-t tablet:border-line mobile:px-2 mobile:text-[13px] print:text-[#111]",
                SplitCodeContent {
                    marker,
                    body,
                    long_text: text,
                    semantic_spans,
                    changed_text_tone,
                    long_line_character_count,
                }
            }
        },
        SplitCellPresentation::Removed => rsx! {
            code { class: "min-w-0 border-0 bg-del-bg px-3 text-[14px] text-code whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 mobile:text-[13px] print:text-[#111]",
                SplitCodeContent {
                    marker,
                    body,
                    long_text: text,
                    semantic_spans,
                    changed_text_tone,
                    long_line_character_count,
                }
            }
        },
        SplitCellPresentation::Added => rsx! {
            code { class: "min-w-0 border-0 bg-add-bg px-3 text-[14px] text-code whitespace-pre-wrap [overflow-wrap:anywhere] tablet:border-t tablet:border-line mobile:px-2 mobile:text-[13px] print:text-[#111]",
                SplitCodeContent {
                    marker,
                    body,
                    long_text: text,
                    semantic_spans,
                    changed_text_tone,
                    long_line_character_count,
                }
            }
        },
    }
}

#[component]
fn SplitCodeContent(
    marker: Option<char>,
    body: String,
    long_text: String,
    semantic_spans: Vec<SemanticTextSpan>,
    changed_text_tone: ChangedTextTone,
    long_line_character_count: Option<usize>,
) -> Element {
    if let Some(character_count) = long_line_character_count {
        return rsx! {
            LongLine { text: long_text, character_count }
        };
    }

    rsx! {
        SplitMarker { marker }
        SemanticText { text: body, semantic_spans, changed_text_tone }
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
    rsx! {
        SplitGutter { side, number: None }
        match side {
            SplitSide::Old => rsx! {
                code { class: "min-w-0 border-0 bg-sunk px-3 text-[14px] text-code whitespace-pre-wrap [overflow-wrap:anywhere] mobile:px-2 mobile:text-[13px] print:text-[#111]" }
            },
            SplitSide::New => rsx! {
                code { class: "min-w-0 border-0 bg-sunk px-3 text-[14px] text-code whitespace-pre-wrap [overflow-wrap:anywhere] tablet:border-t tablet:border-line mobile:px-2 mobile:text-[13px] print:text-[#111]" }
            },
        }
    }
}

#[component]
fn CodeCellContent(
    text: String,
    semantic_spans: Vec<SemanticTextSpan>,
    changed_text_tone: ChangedTextTone,
    long_line_character_count: Option<usize>,
) -> Element {
    if let Some(character_count) = long_line_character_count {
        return rsx! {
            LongLine { text, character_count }
        };
    }

    rsx! {
        SemanticText { text, semantic_spans, changed_text_tone }

    }
}

#[component]
fn LongLine(text: String, character_count: usize) -> Element {
    let mut expanded = use_signal(|| false);
    rsx! {
        span { class: "flex items-baseline gap-2",
            LongLineText { text, expanded: expanded() }
            LongLineControl {
                character_count,
                expanded: expanded(),
                on_toggle: move |_| expanded.toggle(),
            }
        }
    }
}

#[component]
fn LongLineText(text: String, expanded: bool) -> Element {
    if expanded {
        rsx! {
            span { class: "min-w-0 flex-1 overflow-x-auto text-clip whitespace-pre [scrollbar-color:var(--acc)_transparent] [scrollbar-width:thin] [&::-webkit-scrollbar]:size-1.5 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-[linear-gradient(125deg,var(--acc),var(--acc-2))] [&::-webkit-scrollbar-thumb:hover]:bg-[linear-gradient(125deg,var(--acc-2),var(--acc))]",
                "{text}"
            }
        }
    } else {
        rsx! {
            span { class: "min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-pre [scrollbar-color:var(--acc)_transparent] [scrollbar-width:thin] [&::-webkit-scrollbar]:size-1.5 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-[linear-gradient(125deg,var(--acc),var(--acc-2))] [&::-webkit-scrollbar-thumb:hover]:bg-[linear-gradient(125deg,var(--acc-2),var(--acc))]",
                "{text}"
            }
        }
    }
}

#[component]
fn LongLineControl(character_count: usize, expanded: bool, on_toggle: EventHandler<()>) -> Element {
    rsx! {
        Button {
            class: "flex-none select-none [font:inherit]",
            size: ButtonSize::Inline,
            variant: ButtonVariant::Secondary,
            aria_expanded: expanded.to_string(),
            onclick: move |_| on_toggle.call(()),
            "\u{22ef} {character_count} chars"
        }
    }
}

#[component]
fn SemanticText(
    text: String,
    semantic_spans: Vec<SemanticTextSpan>,
    changed_text_tone: ChangedTextTone,
) -> Element {
    let spans = semantic_spans
        .into_iter()
        .map(|span| {
            (
                span.text(&text).to_owned(),
                span.syntax_class(),
                span.change(),
            )
        })
        .collect::<Vec<_>>();
    rsx! {
        if spans.is_empty() {
            "\u{00a0}"
        } else {
            for (index, (text, syntax_class, change)) in spans.into_iter().enumerate() {
                SemanticSpan {
                    key: "{index}",
                    text,
                    syntax_class,
                    changed_text_tone: if change == SemanticTextChange::Changed { changed_text_tone } else { ChangedTextTone::None },
                }
            }
        }
    }
}

#[component]
fn SemanticSpan(
    text: String,
    syntax_class: Option<SyntaxTokenClass>,
    changed_text_tone: ChangedTextTone,
) -> Element {
    match changed_text_tone {
        ChangedTextTone::None => rsx! {
            SyntaxSpan { text, syntax_class }
        },
        ChangedTextTone::Removed => rsx! {
            span { class: "rounded-[2px] bg-[color-mix(in_srgb,var(--del)_34%,transparent)]",
                SyntaxSpan { text, syntax_class }
            }
        },
        ChangedTextTone::Added => rsx! {
            span { class: "rounded-[2px] bg-[color-mix(in_srgb,var(--add)_34%,transparent)]",
                SyntaxSpan { text, syntax_class }
            }
        },
    }
}

#[component]
fn SyntaxSpan(text: String, syntax_class: Option<SyntaxTokenClass>) -> Element {
    match syntax_class {
        None => rsx! {
            span { "{text}" }
        },
        Some(SyntaxTokenClass::Keyword) => {
            rsx! {
                span { class: "text-[var(--sy-kw)]", "{text}" }
            }
        }
        Some(SyntaxTokenClass::String) => {
            rsx! {
                span { class: "text-[var(--sy-str)]", "{text}" }
            }
        }
        Some(SyntaxTokenClass::Comment) => {
            rsx! {
                span { class: "text-[var(--sy-com)]", "{text}" }
            }
        }
        Some(SyntaxTokenClass::Type) => {
            rsx! {
                span { class: "text-[var(--sy-typ)]", "{text}" }
            }
        }
        Some(SyntaxTokenClass::Function) => {
            rsx! {
                span { class: "text-[var(--sy-fn)]", "{text}" }
            }
        }
        Some(SyntaxTokenClass::Number) => {
            rsx! {
                span { class: "text-[var(--sy-num)]", "{text}" }
            }
        }
        Some(SyntaxTokenClass::Constant) => {
            rsx! {
                span { class: "text-[var(--sy-con)]", "{text}" }
            }
        }
        Some(SyntaxTokenClass::Operator) => {
            rsx! {
                span { class: "text-[var(--sy-op)]", "{text}" }
            }
        }
        Some(SyntaxTokenClass::Tag) => {
            rsx! {
                span { class: "text-[var(--sy-tag)]", "{text}" }
            }
        }
        Some(SyntaxTokenClass::Variable) => {
            rsx! {
                span { class: "text-[var(--sy-var)]", "{text}" }
            }
        }
    }
}

fn non_breaking_if_empty(text: &str) -> String {
    if text.is_empty() {
        "\u{00a0}".to_owned()
    } else {
        text.to_owned()
    }
}
