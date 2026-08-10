use crate::{
    DiffRow, DiffRowKind, DiffSide, SyntaxDefinition, SyntaxDiagnostic, syntax::SideHighlighter,
};

pub(crate) fn attach_syntax_tokens(
    rows: &mut [DiffRow],
    syntax: &SyntaxDefinition,
) -> Vec<SyntaxDiagnostic> {
    let mut old_side = SideHighlighter::new(syntax);
    let mut new_side = SideHighlighter::new(syntax);
    let mut diagnostics = Vec::new();

    for row in rows {
        if row.long_line_character_count().is_some() {
            continue;
        }
        let body = row.body().to_owned();
        let tokens = match row.kind() {
            DiffRowKind::Meta | DiffRowKind::Hunk => Vec::new(),
            DiffRowKind::Removed => tokens(&mut old_side, &body, DiffSide::Old, &mut diagnostics),
            DiffRowKind::Added => tokens(&mut new_side, &body, DiffSide::New, &mut diagnostics),
            DiffRowKind::Context => {
                let _ = tokens(&mut old_side, &body, DiffSide::Old, &mut diagnostics);
                tokens(&mut new_side, &body, DiffSide::New, &mut diagnostics)
            }
        };
        row.set_syntax_tokens(tokens);
    }

    diagnostics
}

fn tokens(
    highlighter: &mut SideHighlighter,
    body: &str,
    side: DiffSide,
    diagnostics: &mut Vec<SyntaxDiagnostic>,
) -> Vec<crate::SyntaxToken> {
    match highlighter.tokens(body) {
        Ok(tokens) => tokens,
        Err(message) => {
            diagnostics.push(SyntaxDiagnostic::new(side, message));
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{DiffParser, DiffRowKind, SyntaxTokenClass, bundled_syntax_catalog};

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn context_feeds_both_sides() {
        let syntax = bundled_syntax_catalog()
            .expect("bundled syntax catalog should load")
            .syntax_for_path("a.rs");
        let parsed = DiffParser::new().with_syntax(syntax).parse(&lines(&[
            "@@ -1,4 +1,4 @@",
            " /* open",
            "-old inside",
            "+new inside",
            " */",
        ]));

        for row in &parsed.rows()[2..=3] {
            assert!(matches!(
                row.kind(),
                DiffRowKind::Removed | DiffRowKind::Added
            ));
            assert!(!row.syntax_tokens().is_empty());
            assert!(
                row.syntax_tokens()
                    .iter()
                    .all(|token| token.class() == SyntaxTokenClass::Comment)
            );
        }
    }
}
