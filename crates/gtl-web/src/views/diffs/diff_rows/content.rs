use dioxus::prelude::*;

use crate::{
    entities::diffs::{ViewerCodeLine, ViewerSplitRow, ViewerSyntaxClass, ViewerUnifiedRow},
    shared::ui::{Button, ButtonSize, ButtonVariant},
};

const LONG_LINE_TEXT_CLASSES: &str = "min-w-0 flex-1 whitespace-pre [scrollbar-color:var(--acc)_transparent] [scrollbar-width:thin] [&::-webkit-scrollbar]:size-1.5 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-[linear-gradient(125deg,var(--acc),var(--acc-2))] [&::-webkit-scrollbar-thumb:hover]:bg-[linear-gradient(125deg,var(--acc-2),var(--acc))]";
const LONG_LINE_COLLAPSED_CLASSES: &str = "overflow-hidden text-ellipsis";
const LONG_LINE_EXPANDED_CLASSES: &str = "overflow-x-auto text-clip";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChangedTextTone {
    None,
    Removed,
    Added,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum CodeLineSource {
    Unified(ReadStore<ViewerUnifiedRow>),
    SplitContext(ReadStore<ViewerSplitRow>),
    SplitOld(ReadStore<ViewerSplitRow>),
    SplitNew(ReadStore<ViewerSplitRow>),
}

impl CodeLineSource {
    fn with<R>(self, read: impl FnOnce(Option<&ViewerCodeLine>) -> R) -> R {
        match self {
            Self::Unified(row) => {
                let row = row.read();
                let code = match &*row {
                    ViewerUnifiedRow::Context(source)
                    | ViewerUnifiedRow::Added(source)
                    | ViewerUnifiedRow::Removed(source) => Some(&source.code),
                    ViewerUnifiedRow::Meta(_) | ViewerUnifiedRow::Hunk(_) => None,
                };
                read(code)
            }
            Self::SplitContext(row) => {
                let row = row.read();
                let code = match &*row {
                    ViewerSplitRow::Context { code, .. } => Some(code),
                    ViewerSplitRow::Meta(_)
                    | ViewerSplitRow::Hunk(_)
                    | ViewerSplitRow::Pair { .. } => None,
                };
                read(code)
            }
            Self::SplitOld(row) => {
                let row = row.read();
                let code = match &*row {
                    ViewerSplitRow::Pair {
                        old: Some(cell), ..
                    } => Some(&cell.code),
                    ViewerSplitRow::Meta(_)
                    | ViewerSplitRow::Hunk(_)
                    | ViewerSplitRow::Context { .. }
                    | ViewerSplitRow::Pair { old: None, .. } => None,
                };
                read(code)
            }
            Self::SplitNew(row) => {
                let row = row.read();
                let code = match &*row {
                    ViewerSplitRow::Pair {
                        new: Some(cell), ..
                    } => Some(&cell.code),
                    ViewerSplitRow::Meta(_)
                    | ViewerSplitRow::Hunk(_)
                    | ViewerSplitRow::Context { .. }
                    | ViewerSplitRow::Pair { new: None, .. } => None,
                };
                read(code)
            }
        }
    }
}

#[component]
pub(super) fn CodeCellContent(
    source: CodeLineSource,
    marker: Option<char>,
    changed_text_tone: ChangedTextTone,
    artifact_enhancement: bool,
    copy_text: bool,
) -> Element {
    source.with(|code| {
        let Some(code) = code else {
            return rsx! {};
        };
        if let Some(character_count) = code.long_line_character_count {
            return rsx! {
                LongLine {
                    source,
                    marker,
                    character_count,
                    artifact_enhancement,
                    copy_text,
                }
            };
        }

        rsx! {
            if let Some(marker) = marker {
                span { "{marker}" }
            }
            SemanticText { source, changed_text_tone, copy_text }
        }
    })
}

#[component]
pub(super) fn LongLine(
    source: CodeLineSource,
    marker: Option<char>,
    character_count: usize,
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
                source,
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
    source: CodeLineSource,
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
    source.with(|code| {
        let Some(code) = code else {
            return rsx! {};
        };
        let text = code.text.as_str();
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
    })
}

#[component]
fn LongLineControl(
    character_count: usize,
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
    source: CodeLineSource,
    changed_text_tone: ChangedTextTone,
    copy_text: bool,
) -> Element {
    source.with(|code| {
        let Some(code) = code else {
            return rsx! {};
        };
        let spans = &code.spans;
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
                for (index, semantic_span) in spans.iter().enumerate() {
                    {
                        let syntax_class = syntax_classes(semantic_span.syntax_class);
                        let changed_class = changed_text_classes(
                            if semantic_span.changed { changed_text_tone } else { ChangedTextTone::None },
                        );
                        rsx! {
                            span { key: "{index}", class: "{syntax_class} {changed_class}", "{semantic_span.text}" }
                        }
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
    })
}

const fn changed_text_classes(changed_text_tone: ChangedTextTone) -> &'static str {
    match changed_text_tone {
        ChangedTextTone::None => "",
        ChangedTextTone::Removed => "rounded-xs bg-[color-mix(in_srgb,var(--del)_34%,transparent)]",
        ChangedTextTone::Added => "rounded-xs bg-[color-mix(in_srgb,var(--add)_34%,transparent)]",
    }
}

const fn syntax_classes(syntax_class: Option<ViewerSyntaxClass>) -> &'static str {
    match syntax_class {
        None => "",
        Some(ViewerSyntaxClass::Keyword) => "text-[var(--sy-kw)]",
        Some(ViewerSyntaxClass::String) => "text-[var(--sy-str)]",
        Some(ViewerSyntaxClass::Comment) => "text-[var(--sy-com)]",
        Some(ViewerSyntaxClass::Type) => "text-[var(--sy-typ)]",
        Some(ViewerSyntaxClass::Function) => "text-[var(--sy-fn)]",
        Some(ViewerSyntaxClass::Number) => "text-[var(--sy-num)]",
        Some(ViewerSyntaxClass::Constant) => "text-[var(--sy-con)]",
        Some(ViewerSyntaxClass::Operator) => "text-[var(--sy-op)]",
        Some(ViewerSyntaxClass::Tag) => "text-[var(--sy-tag)]",
        Some(ViewerSyntaxClass::Variable) => "text-[var(--sy-var)]",
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

    #[component]
    fn EmptySemanticText() -> Element {
        let row = use_store(|| {
            ViewerUnifiedRow::Added(gtl_wire::viewer::ViewerUnifiedSourceRow {
                old_line_number: None,
                new_line_number: Some(1),
                code: ViewerCodeLine {
                    text: String::new(),
                    spans: Vec::new(),
                    long_line_character_count: None,
                },
            })
        });
        let row: ReadStore<ViewerUnifiedRow> = row.into();
        rsx! {
            SemanticText {
                source: CodeLineSource::Unified(row),
                changed_text_tone: ChangedTextTone::None,
                copy_text: true,
            }
        }
    }

    #[test]
    fn copy_text_keeps_an_empty_source_line_distinct_from_its_visual_placeholder() {
        let html = dioxus_ssr::render_element(rsx! {
            EmptySemanticText {}
        });

        assert!(html.contains(r#"<span data-gtl-copy-text=""></span>"#));
        assert!(html.contains('\u{00a0}'));
    }
}
