//! Escaping and small text helpers shared by the row renderers and the layout
//! templates.

pub(crate) fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        push_escaped(&mut out, ch);
    }
    out
}

// ! Single source of the HTML escape mapping, char by char, so the intra-line span renderer
// ! can interleave `<span>` markers between escaped chars without re-escaping whole substrings.
pub(crate) fn push_escaped(out: &mut String, ch: char) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        _ => out.push(ch),
    }
}

pub(crate) fn html_or_nbsp(raw: &str) -> String {
    let escaped = escape_html(raw);
    if escaped.is_empty() {
        "&nbsp;".to_string()
    } else {
        escaped
    }
}

pub(crate) fn slug(s: &str) -> String {
    let mut body = String::new();
    let mut last_dash = false;

    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            body.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            body.push('-');
            last_dash = true;
        }
    }

    while body.starts_with('-') {
        body.remove(0);
    }
    while body.ends_with('-') {
        body.pop();
    }

    format!("f-{body}")
}

pub(crate) fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_html_escapes_metacharacters() {
        assert_eq!(escape_html("&<>\""), "&amp;&lt;&gt;&quot;");
    }

    #[test]
    fn slug_normalizes_file_paths_to_anchor_ids() {
        assert_eq!(slug("src/a b.rs"), "f-src-a-b-rs");
    }

    #[test]
    fn slug_preserves_prefix_when_normalized_body_is_empty() {
        assert_eq!(slug("---"), "f-");
    }
}
