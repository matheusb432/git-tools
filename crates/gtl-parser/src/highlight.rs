use crate::{
    DiffRow, DiffRowKind, DiffSide, SyntaxDefinition, SyntaxDiagnostic, syntax::SideHighlighter,
};

pub(crate) struct DiffSyntaxHighlighter {
    old_side: SideHighlighter,
    new_side: SideHighlighter,
}

impl DiffSyntaxHighlighter {
    pub(crate) fn new(syntax: &SyntaxDefinition) -> Self {
        Self {
            old_side: SideHighlighter::new(syntax),
            new_side: SideHighlighter::new(syntax),
        }
    }

    pub(crate) fn attach(&mut self, rows: &mut [DiffRow]) -> Vec<SyntaxDiagnostic> {
        let mut diagnostics = Vec::new();

        for row in rows {
            if row.long_line_character_count().is_some() {
                continue;
            }
            let body = row.body().to_owned();
            let tokens = match row.kind() {
                DiffRowKind::Meta | DiffRowKind::Hunk => Vec::new(),
                DiffRowKind::Removed => {
                    tokens(&mut self.old_side, &body, DiffSide::Old, &mut diagnostics)
                }
                DiffRowKind::Added => {
                    tokens(&mut self.new_side, &body, DiffSide::New, &mut diagnostics)
                }
                DiffRowKind::Context => {
                    let _ = tokens(&mut self.old_side, &body, DiffSide::Old, &mut diagnostics);
                    tokens(&mut self.new_side, &body, DiffSide::New, &mut diagnostics)
                }
            };
            row.set_syntax_tokens(tokens);
        }

        diagnostics
    }
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

    #[test]
    fn syntax_state_crosses_streaming_page_boundaries() {
        let syntax = bundled_syntax_catalog()
            .expect("bundled syntax catalog should load")
            .syntax_for_path("a.rs");
        let parser = DiffParser::new().with_syntax(syntax);
        let mut stream = parser.stream();

        let first = stream.push(&lines(&["@@ -1,4 +1,4 @@", " /* open"]));
        let second = stream.push(&lines(&["-old inside", "+new inside", " */"]));

        assert!(first.syntax_diagnostics().is_empty());
        assert!(second.syntax_diagnostics().is_empty());
        for row in &second.rows()[..2] {
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
