use crate::{CharacterSpan, SyntaxToken, SyntaxTokenClass};

/// Whether a semantic source span belongs to an intraline change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTextChange {
    Unchanged,
    Changed,
}

/// One flat semantic range within source text.
///
/// Byte offsets make the range directly sliceable by renderers. Syntax and
/// intraline semantics are combined here so consumers do not need to rescan
/// character-indexed parser output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticTextSpan {
    byte_start: usize,
    byte_end: usize,
    syntax_class: Option<SyntaxTokenClass>,
    change: SemanticTextChange,
}

impl SemanticTextSpan {
    /// Returns the source text covered by this span.
    ///
    /// # Panics
    ///
    /// Panics when `text` is not the source body that produced this span.
    pub fn text<'text>(&self, text: &'text str) -> &'text str {
        &text[self.byte_start..self.byte_end]
    }

    /// Returns the syntax class active across the span.
    pub const fn syntax_class(self) -> Option<SyntaxTokenClass> {
        self.syntax_class
    }

    /// Returns the span's intraline-change state.
    pub const fn change(self) -> SemanticTextChange {
        self.change
    }
}

pub(crate) fn semantic_text_spans(
    text: &str,
    syntax_tokens: &[SyntaxToken],
    intraline_spans: &[CharacterSpan],
) -> Vec<SemanticTextSpan> {
    let mut output = Vec::new();
    let mut syntax_index = 0usize;
    let mut intraline_index = 0usize;
    let mut active = None::<(usize, Option<SyntaxTokenClass>, SemanticTextChange)>;

    for (character_index, (byte_index, _)) in text.char_indices().enumerate() {
        while syntax_tokens
            .get(syntax_index)
            .is_some_and(|token| token.end().into_inner() <= character_index)
        {
            syntax_index += 1;
        }
        while intraline_spans
            .get(intraline_index)
            .is_some_and(|span| span.end().into_inner() <= character_index)
        {
            intraline_index += 1;
        }

        let syntax_class = syntax_tokens.get(syntax_index).and_then(|token| {
            (character_index >= token.start().into_inner()
                && character_index < token.end().into_inner())
            .then(|| token.class())
        });
        let change = if intraline_spans.get(intraline_index).is_some_and(|span| {
            character_index >= span.start().into_inner()
                && character_index < span.end().into_inner()
        }) {
            SemanticTextChange::Changed
        } else {
            SemanticTextChange::Unchanged
        };

        match active {
            Some((start, class, active_change))
                if class != syntax_class || active_change != change =>
            {
                output.push(SemanticTextSpan {
                    byte_start: start,
                    byte_end: byte_index,
                    syntax_class: class,
                    change: active_change,
                });
                active = Some((byte_index, syntax_class, change));
            }
            None => active = Some((byte_index, syntax_class, change)),
            Some(_) => {}
        }
    }

    if let Some((start, syntax_class, change)) = active {
        output.push(SemanticTextSpan {
            byte_start: start,
            byte_end: text.len(),
            syntax_class,
            change,
        });
    }
    output
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "syntax")]
    use crate::SyntaxLanguage;
    use crate::{DiffParser, SplitDiffRow};

    #[cfg(feature = "syntax")]
    #[test]
    fn parser_owned_spans_flatten_syntax_and_intraline_boundaries() {
        let parsed = DiffParser::new()
            .with_syntax(Some(SyntaxLanguage::Rust))
            .parse(&[
                "@@ -1 +1 @@".to_owned(),
                "-let value = 1;".to_owned(),
                "+let value = 2;".to_owned(),
            ]);
        let SplitDiffRow::Pair { old: Some(old), .. } = &parsed.split_rows()[1] else {
            panic!("expected a paired changed row");
        };
        let body = old.body();
        let spans = old.semantic_spans();

        assert_eq!(
            spans
                .iter()
                .map(|span| (span.text(body), span.syntax_class(), span.change(),))
                .collect::<Vec<_>>(),
            vec![
                (
                    "let",
                    Some(crate::SyntaxTokenClass::Keyword),
                    super::SemanticTextChange::Unchanged,
                ),
                (" value = ", None, super::SemanticTextChange::Unchanged),
                (
                    "1",
                    Some(crate::SyntaxTokenClass::Constant),
                    super::SemanticTextChange::Changed,
                ),
                (";", None, super::SemanticTextChange::Unchanged),
            ]
        );
    }

    #[test]
    fn parser_owned_spans_slice_unicode_by_bytes_without_losing_characters() {
        let parsed = DiffParser::new().parse(&[
            "@@ -1 +1 @@".to_owned(),
            "-ação = 1".to_owned(),
            "+ação = 2".to_owned(),
        ]);
        let SplitDiffRow::Pair { old: Some(old), .. } = &parsed.split_rows()[1] else {
            panic!("expected a paired changed row");
        };

        assert_eq!(
            old.semantic_spans()
                .iter()
                .map(|span| span.text(old.body()))
                .collect::<String>(),
            "ação = 1"
        );
        assert!(
            old.semantic_spans()
                .last()
                .is_some_and(|span| span.change() == super::SemanticTextChange::Changed)
        );
    }
}
