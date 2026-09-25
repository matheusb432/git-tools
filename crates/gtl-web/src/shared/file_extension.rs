#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileExtension(String);

/// Reports input that is not one final file extension, such as `lock` or `.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InvalidFileExtension;

impl FileExtension {
    pub(crate) fn parse(raw: &str) -> Result<Self, InvalidFileExtension> {
        let value = raw.trim().trim_start_matches('.').to_lowercase();
        if value.is_empty()
            || value.len() > 255
            || value.contains(['.', '/', '\\', '*', '?', '[', ']', '\0'])
            || value.chars().any(char::is_whitespace)
        {
            return Err(InvalidFileExtension);
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::FileExtension;

    #[test]
    fn extension_input_normalizes_dots_case_and_spaces() -> Result<(), super::InvalidFileExtension>
    {
        assert_eq!(FileExtension::parse(" .JSON ")?.as_str(), "json");
        assert_eq!(FileExtension::parse("lock")?.as_str(), "lock");
        Ok(())
    }

    #[test]
    fn extension_input_rejects_paths_patterns_and_compound_extensions() {
        for input in [
            "",
            ".",
            "*.lock",
            "Cargo.lock",
            "src/lock",
            "a\\b",
            "a b",
            "[a]",
            "a?",
        ] {
            assert!(FileExtension::parse(input).is_err(), "accepted {input:?}");
        }
        assert!(FileExtension::parse(&"x".repeat(256)).is_err());
    }
}
