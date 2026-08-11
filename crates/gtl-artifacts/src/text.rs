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
}
