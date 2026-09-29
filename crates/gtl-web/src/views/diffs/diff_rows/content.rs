use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;

use crate::{
    entities::diffs::{ViewerCodeLine, ViewerCodeSpan, ViewerSyntaxClass},
    shared::i18n::t,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChangedTextTone {
    None,
    Removed,
    Added,
}

pub(super) fn code_cell_content(
    code: &ViewerCodeLine,
    marker: Option<char>,
    changed_text_tone: ChangedTextTone,
    copy_text: bool,
    language: ViewerLanguage,
) -> Element {
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
        {semantic_text(code, changed_text_tone, copy_text)}
    }
}

fn semantic_text(
    code: &ViewerCodeLine,
    changed_text_tone: ChangedTextTone,
    copy_text: bool,
) -> Element {
    let spans = &code.spans;
    if copy_text && spans.is_empty() {
        return rsx! {
            span { "data-gtl-copy-text": "" }
            "\u{00a0}"
        };
    }

    let content = if spans.is_empty() {
        rsx! { "\u{00a0}" }
    } else {
        let tokens = spans
            .iter()
            .map(|semantic_span| semantic_token(&code.text, semantic_span, changed_text_tone));
        rsx! {
            {tokens}
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

fn semantic_token(
    line: &str,
    semantic_span: &ViewerCodeSpan,
    changed_text_tone: ChangedTextTone,
) -> Element {
    let text = semantic_span.text(line).unwrap_or_default();
    let syntax_class = syntax_classes(semantic_span.syntax_class);
    let changed_class = changed_text_classes(if semantic_span.changed {
        changed_text_tone
    } else {
        ChangedTextTone::None
    });
    if syntax_class.is_empty() && changed_class.is_empty() {
        return rsx! { "{text}" };
    }
    rsx! {
        span { class: "{syntax_class} {changed_class}", "{text}" }
    }
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
    use crate::test_support::code_line;

    #[test]
    fn copy_text_keeps_an_empty_source_line_distinct_from_its_visual_placeholder() {
        let code = ViewerCodeLine {
            text: String::new(),
            spans: Vec::new(),
            omitted_character_count: None,
        };
        let html = dioxus_ssr::render_element(semantic_text(&code, ChangedTextTone::None, true));

        assert!(html.contains(r#"<span data-gtl-copy-text=""></span>"#));
        assert!(html.contains('\u{00a0}'));
    }

    #[test]
    fn unstyled_tokens_render_as_text_inside_the_copy_boundary() {
        let html = dioxus_ssr::render_element(semantic_text(
            &code_line("plain source", None),
            ChangedTextTone::Added,
            true,
        ));

        assert_eq!(html, r#"<span data-gtl-copy-text="">plain source</span>"#);
    }
}
