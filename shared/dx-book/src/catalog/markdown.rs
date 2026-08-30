use dioxus::prelude::*;
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};

#[component]
pub(super) fn Markdown(source: &'static str) -> Element {
    let rendered = use_memo(use_reactive((&source,), move |(source,)| {
        render_safe_html(source)
    }));

    rsx! { div { dangerous_inner_html: rendered() } }
}

pub(super) fn summary(source: &str, character_count_max: usize) -> String {
    let mut text = String::new();
    for event in Parser::new_ext(source, Options::all()) {
        match event {
            Event::Text(value) | Event::Code(value) => append_summary_text(&mut text, &value),
            Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::Paragraph) => text.push(' '),
            _ => {}
        }
    }
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= character_count_max {
        return normalized;
    }
    let retained_character_count = character_count_max.saturating_sub(3);
    let mut truncated = normalized
        .chars()
        .take(retained_character_count)
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn append_summary_text(summary: &mut String, value: &str) {
    if !summary.ends_with(char::is_whitespace) && !summary.is_empty() {
        summary.push(' ');
    }
    summary.push_str(value);
}

fn render_safe_html(source: &str) -> String {
    let events = Parser::new_ext(source, Options::all()).map(sanitize_event);
    let mut output = String::new();
    html::push_html(&mut output, events);
    output
}

fn sanitize_event(event: Event<'_>) -> Event<'_> {
    match event {
        Event::Html(source) | Event::InlineHtml(source) => Event::Text(source),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: safe_destination(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: safe_destination(dest_url),
            title,
            id,
        }),
        event => event,
    }
}

fn safe_destination(destination: CowStr<'_>) -> CowStr<'_> {
    let normalized = destination
        .chars()
        .filter(|character| !character.is_ascii_control() && !character.is_ascii_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    let explicitly_safe = ["http://", "https://", "mailto:", "#", "/", "./", "../"]
        .iter()
        .any(|prefix| normalized.starts_with(prefix));
    if explicitly_safe || !normalized.contains(':') {
        destination
    } else {
        CowStr::Borrowed("#")
    }
}

#[cfg(test)]
mod tests {
    use super::{render_safe_html, summary};

    #[test]
    fn markdown_escapes_raw_html() {
        let rendered = render_safe_html("Hello <script>alert('no')</script>.");

        assert!(!rendered.contains("<script>"));
        assert!(rendered.contains("&lt;script&gt;"));
    }

    #[test]
    fn markdown_rejects_active_link_schemes() {
        let rendered = render_safe_html("[unsafe](javascript:alert('no'))");

        assert!(!rendered.contains("javascript:"));
        assert!(rendered.contains("href=\"#\""));
    }

    #[test]
    fn summary_removes_markdown_and_bounds_card_copy() {
        let rendered = summary("## Heading\n\nA **long** `description` for a card.", 24);

        assert_eq!(rendered, "Heading A long descri...");
    }
}
