use dioxus::prelude::*;
use gtl_parser::{SemanticTextChange, SemanticTextSpan, SyntaxTokenClass};

use crate::shared::ui::{Button, ButtonSize, ButtonVariant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChangedTextTone {
    None,
    Removed,
    Added,
}

#[component]
pub(super) fn CodeCellContent(
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
pub(super) fn LongLine(text: String, character_count: usize) -> Element {
    let mut expanded = use_signal(|| false);
    rsx! {
        span { class: "flex items-baseline gap-2",
            LongLineText { text, expanded: expanded() }
            LongLineControl {
                character_count,
                expanded: expanded(),
                on_toggle: move |()| expanded.toggle(),
            }
        }
    }
}

#[component]
fn LongLineText(text: String, expanded: bool) -> Element {
    rsx! {
        span {
            class: "min-w-0 flex-1 whitespace-pre [scrollbar-color:var(--acc)_transparent] [scrollbar-width:thin] [&::-webkit-scrollbar]:size-1.5 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-[linear-gradient(125deg,var(--acc),var(--acc-2))] [&::-webkit-scrollbar-thumb:hover]:bg-[linear-gradient(125deg,var(--acc-2),var(--acc))]",
            class: if expanded { "overflow-x-auto text-clip" } else { "overflow-hidden text-ellipsis" },
            "{text}"
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
pub(super) fn SemanticText(
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
            span { class: "rounded-xs bg-[color-mix(in_srgb,var(--del)_34%,transparent)]",
                SyntaxSpan { text, syntax_class }
            }
        },
        ChangedTextTone::Added => rsx! {
            span { class: "rounded-xs bg-[color-mix(in_srgb,var(--add)_34%,transparent)]",
                SyntaxSpan { text, syntax_class }
            }
        },
    }
}

#[component]
fn SyntaxSpan(text: String, syntax_class: Option<SyntaxTokenClass>) -> Element {
    let classes = match syntax_class {
        None => "",
        Some(SyntaxTokenClass::Keyword) => "text-[var(--sy-kw)]",
        Some(SyntaxTokenClass::String) => "text-[var(--sy-str)]",
        Some(SyntaxTokenClass::Comment) => "text-[var(--sy-com)]",
        Some(SyntaxTokenClass::Type) => "text-[var(--sy-typ)]",
        Some(SyntaxTokenClass::Function) => "text-[var(--sy-fn)]",
        Some(SyntaxTokenClass::Number) => "text-[var(--sy-num)]",
        Some(SyntaxTokenClass::Constant) => "text-[var(--sy-con)]",
        Some(SyntaxTokenClass::Operator) => "text-[var(--sy-op)]",
        Some(SyntaxTokenClass::Tag) => "text-[var(--sy-tag)]",
        Some(SyntaxTokenClass::Variable) => "text-[var(--sy-var)]",
    };

    rsx! {
        span { class: "{classes}", "{text}" }
    }
}

pub(super) fn non_breaking_if_empty(text: &str) -> String {
    if text.is_empty() {
        "\u{00a0}".to_owned()
    } else {
        text.to_owned()
    }
}
