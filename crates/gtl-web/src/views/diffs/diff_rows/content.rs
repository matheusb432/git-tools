use dioxus::prelude::*;

use crate::{
    entities::diffs::{ViewerCodeLine, ViewerSplitRow, ViewerSyntaxClass, ViewerUnifiedRow},
    shared::i18n::{t, use_language},
};

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
    copy_text: bool,
) -> Element {
    let language = use_language();
    source.with(|code| {
        let Some(code) = code else {
            return rsx! {};
        };
        if let Some(omitted) = code.omitted_character_count {
            let omission = t!(language, "diff-line-omitted", count = omitted);
            let copy_text = copy_text.then_some("");
            let text = code.text.as_str();
            return rsx! {
                span { class: "diff-truncated-line",
                    if let Some(marker) = marker {
                        span { "{marker}" }
                    }
                    span {
                        class: "diff-truncated-source",
                        "data-gtl-copy-text": copy_text,
                        span { class: "diff-truncated-text", "{text}" }
                        span { class: "diff-line-omission", " {omission}" }
                    }
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
                        let text = semantic_span.text(&code.text).unwrap_or_default();
                        let syntax_class = syntax_classes(semantic_span.syntax_class);
                        let changed_class = changed_text_classes(
                            if semantic_span.changed { changed_text_tone } else { ChangedTextTone::None },
                        );
                        rsx! {
                            span { key: "{index}", class: "{syntax_class} {changed_class}", "{text}" }
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
        ChangedTextTone::Removed => "diff-text-change-removed",
        ChangedTextTone::Added => "diff-text-change-added",
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
                    omitted_character_count: None,
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
