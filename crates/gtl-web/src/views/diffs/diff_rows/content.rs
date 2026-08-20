use dioxus::prelude::*;
use gtl_parser::{CharacterCount, SemanticTextChange, SemanticTextSpan, SyntaxTokenClass};

use crate::shared::ui::{Button, ButtonSize, ButtonVariant};

const LONG_LINE_TEXT_CLASSES: &str = "min-w-0 flex-1 whitespace-pre [scrollbar-color:var(--acc)_transparent] [scrollbar-width:thin] [&::-webkit-scrollbar]:size-1.5 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-[linear-gradient(125deg,var(--acc),var(--acc-2))] [&::-webkit-scrollbar-thumb:hover]:bg-[linear-gradient(125deg,var(--acc-2),var(--acc))]";
const LONG_LINE_COLLAPSED_CLASSES: &str = "overflow-hidden text-ellipsis";
const LONG_LINE_EXPANDED_CLASSES: &str = "overflow-x-auto text-clip";

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
    long_line_character_count: Option<CharacterCount>,
    artifact_enhancement: bool,
    copy_text: bool,
) -> Element {
    if let Some(character_count) = long_line_character_count {
        return rsx! {
            LongLine {
                text,
                marker: None,
                character_count,
                artifact_enhancement,
                copy_text,
            }
        };
    }

    rsx! {
        SemanticText {
            text,
            semantic_spans,
            changed_text_tone,
            copy_text,
        }
    }
}

#[component]
pub(super) fn LongLine(
    text: String,
    marker: Option<char>,
    character_count: CharacterCount,
    artifact_enhancement: bool,
    copy_text: bool,
) -> Element {
    let mut expanded = use_signal(|| false);
    let is_expanded = expanded();
    let artifact_long_line = artifact_enhancement.then_some("");
    let artifact_expanded = artifact_enhancement.then(|| is_expanded.to_string());
    rsx! {
        span {
            class: "flex items-baseline gap-2",
            "data-gtl-long-line": artifact_long_line,
            "data-gtl-expanded": artifact_expanded,
            LongLineText {
                text,
                marker,
                expanded: is_expanded,
                artifact_enhancement,
                copy_text,
            }
            LongLineControl {
                character_count,
                expanded: is_expanded,
                artifact_enhancement,
                on_toggle: move |()| expanded.toggle(),
            }
        }
    }
}

#[component]
fn LongLineText(
    text: String,
    marker: Option<char>,
    expanded: bool,
    artifact_enhancement: bool,
    copy_text: bool,
) -> Element {
    let marker = marker.map(|value| value.to_string());
    let artifact_long_line_text = artifact_enhancement.then_some("");
    let artifact_expanded_classes = artifact_enhancement.then_some(LONG_LINE_EXPANDED_CLASSES);
    let artifact_collapsed_classes = artifact_enhancement.then_some(LONG_LINE_COLLAPSED_CLASSES);
    let overflow_classes = if expanded {
        LONG_LINE_EXPANDED_CLASSES
    } else {
        LONG_LINE_COLLAPSED_CLASSES
    };
    rsx! {
        span {
            class: "{LONG_LINE_TEXT_CLASSES}",
            class: "{overflow_classes}",
            "data-gtl-long-line-text": artifact_long_line_text,
            "data-gtl-expanded-classes": artifact_expanded_classes,
            "data-gtl-collapsed-classes": artifact_collapsed_classes,
            {marker}
            if copy_text {
                span { "data-gtl-copy-text": "", "{text}" }
            } else {
                "{text}"
            }
        }
    }
}

#[component]
fn LongLineControl(
    character_count: CharacterCount,
    expanded: bool,
    artifact_enhancement: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let artifact_action = artifact_enhancement.then_some("toggle-long-line");
    rsx! {
        Button {
            class: "flex-none select-none [font:inherit]",
            size: ButtonSize::Inline,
            variant: ButtonVariant::Secondary,
            aria_expanded: expanded.to_string(),
            "data-gtl-action": artifact_action,
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
    copy_text: bool,
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
    if copy_text && spans.is_empty() {
        return rsx! {
            span { "data-gtl-copy-text": "" }
            "\u{00a0}"
        };
    }

    let content = rsx! {
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
    };
    if copy_text {
        rsx! {
            span { "data-gtl-copy-text": "", {content} }
        }
    } else {
        content
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_text_keeps_an_empty_source_line_distinct_from_its_visual_placeholder() {
        let html = dioxus_ssr::render_element(rsx! {
            SemanticText {
                text: String::new(),
                semantic_spans: Vec::new(),
                changed_text_tone: ChangedTextTone::None,
                copy_text: true,
            }
        });

        assert!(html.contains(r#"<span data-gtl-copy-text=""></span>"#));
        assert!(html.contains('\u{00a0}'));
    }
}
