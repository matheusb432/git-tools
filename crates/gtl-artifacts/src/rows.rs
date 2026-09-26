use std::fmt::{self, Write as _};

use gtl_application::diffs::FileDiff;
use gtl_models::viewer::DiffDensity;
use gtl_parser::{
    DiffParser, DiffRow, DiffRowKind, SemanticTextChange, SyntaxLanguage, SyntaxTokenClass,
};

use super::{document::Escaped, labels::Labels};

const LINE_PREVIEW_CHARACTERS_MAX: usize = 500;

pub(super) fn render(
    output: &mut String,
    file: &FileDiff,
    density: DiffDensity,
    labels: &Labels,
) -> fmt::Result {
    let lines = match density {
        DiffDensity::Full => file.full_lines.as_ref().unwrap_or(&file.lines),
        DiffDensity::Compact => &file.lines,
    };
    let parsed = DiffParser::new()
        .with_syntax(SyntaxLanguage::from_path(file.path.as_path()))
        .parse(lines.iter());
    write!(
        output,
        "<div class=\"diff-scroll\" tabindex=\"0\" role=\"region\" aria-label=\"{}\"><table class=\"diff\"><thead><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
        Escaped(&file.path.to_string_lossy()),
        labels.old_line,
        labels.new_line,
        labels.change,
        labels.source
    )?;
    for row in parsed.rows() {
        render_row(output, row, labels)?;
    }
    output.push_str("</tbody></table></div>");
    Ok(())
}

fn render_row(output: &mut String, row: &DiffRow, labels: &Labels) -> fmt::Result {
    let (class, marker) = match row.kind() {
        DiffRowKind::Meta => ("meta", ""),
        DiffRowKind::Hunk => ("hunk", ""),
        DiffRowKind::Context => ("context", " "),
        DiffRowKind::Added => ("added", "+"),
        DiffRowKind::Removed => ("removed", "−"),
    };
    write!(output, "<tr class=\"{class}\"><td class=\"number\">")?;
    if let Some(line) = row.old_line_number() {
        write!(output, "{}", line.into_inner())?;
    }
    output.push_str("</td><td class=\"number\">");
    if let Some(line) = row.new_line_number() {
        write!(output, "{}", line.into_inner())?;
    }
    write!(
        output,
        "</td><td class=\"marker\">{marker}</td><td class=\"source\">"
    )?;
    render_source(output, row, labels)?;
    output.push_str("</td></tr>");
    Ok(())
}

fn render_source(output: &mut String, row: &DiffRow, labels: &Labels) -> fmt::Result {
    let text = match row.kind() {
        DiffRowKind::Meta | DiffRowKind::Hunk => row.text(),
        _ => row.body(),
    };
    if let Some((end, _)) = text.char_indices().nth(LINE_PREVIEW_CHARACTERS_MAX) {
        return write!(
            output,
            "<details class=\"long-line\"><summary><code>{}…</code><span class=\"expand-line\">{}</span></summary><code class=\"line-full\">{}</code></details>",
            Escaped(&text[..end]),
            labels.expand_line,
            Escaped(text)
        );
    }
    output.push_str("<code>");
    if row.semantic_spans().is_empty() {
        write!(output, "{}", Escaped(text))?;
    } else {
        for span in row.semantic_spans() {
            let class = span.syntax_class().map(syntax_class);
            let changed = span.change() == SemanticTextChange::Changed;
            let change_class = if changed { " changed" } else { "" };
            if class.is_some() || changed {
                write!(
                    output,
                    "<span class=\"{}{}\">{}</span>",
                    class.unwrap_or_default(),
                    change_class,
                    Escaped(span.text(text))
                )?;
            } else {
                write!(output, "{}", Escaped(span.text(text)))?;
            }
        }
    }
    output.push_str("</code>");
    Ok(())
}

const fn syntax_class(class: SyntaxTokenClass) -> &'static str {
    match class {
        SyntaxTokenClass::Keyword => "keyword",
        SyntaxTokenClass::String => "string",
        SyntaxTokenClass::Comment => "comment",
        SyntaxTokenClass::Type => "type",
        SyntaxTokenClass::Function => "function",
        SyntaxTokenClass::Number => "number-token",
        SyntaxTokenClass::Constant => "constant",
        SyntaxTokenClass::Operator => "operator",
        SyntaxTokenClass::Tag => "tag",
        SyntaxTokenClass::Variable => "variable",
    }
}
