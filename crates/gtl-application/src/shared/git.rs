//! Small pure helpers shared by Git-facing application operations.

/// Returns the last trimmed, non-empty line in captured output.
pub fn last_non_empty_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

#[cfg(test)]
mod tests {
    use super::last_non_empty_line;

    #[test]
    fn last_non_empty_line_finds_the_last_populated_line() {
        assert_eq!(last_non_empty_line("a\n\nb\n \n"), Some("b"));
        assert_eq!(last_non_empty_line("\n\n"), None);
    }
}
