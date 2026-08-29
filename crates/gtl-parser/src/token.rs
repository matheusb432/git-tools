use crate::CharacterOffset;

/// A semantic class assigned to a source-code token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxTokenClass {
    Keyword,
    String,
    Comment,
    Type,
    Function,
    Number,
    Constant,
    Operator,
    Tag,
    Variable,
}

/// A half-open character range assigned to one semantic token class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxToken {
    start: CharacterOffset,
    end: CharacterOffset,
    class: SyntaxTokenClass,
}

impl SyntaxToken {
    #[cfg(feature = "syntax")]
    pub(crate) const fn new(start: usize, end: usize, class: SyntaxTokenClass) -> Self {
        Self {
            start: CharacterOffset::new(start),
            end: CharacterOffset::new(end),
            class,
        }
    }

    /// Returns the first character index covered by the token.
    #[must_use]
    pub const fn start(&self) -> CharacterOffset {
        self.start
    }

    /// Returns the exclusive character index after the token.
    #[must_use]
    pub const fn end(&self) -> CharacterOffset {
        self.end
    }

    /// Returns the token's semantic class.
    #[must_use]
    pub const fn class(&self) -> SyntaxTokenClass {
        self.class
    }
}
