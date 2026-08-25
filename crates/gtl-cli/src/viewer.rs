//! Explicit viewer opt-out shared by CLI commands.

/// Returns true if the given env-var value is a truthy `NO_OPEN` sentinel
/// (`1 | true | TRUE | yes | YES`, after trimming).
pub(crate) fn is_no_open(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

pub(crate) fn no_open_requested() -> bool {
    is_no_open(std::env::var("GIT_TOOLS_NO_OPEN").ok().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_no_open_accepts_truthy_values() {
        assert!(is_no_open(Some("1")));
        assert!(is_no_open(Some("true")));
        assert!(is_no_open(Some("TRUE")));
        assert!(is_no_open(Some("yes")));
        assert!(is_no_open(Some("YES")));
        assert!(!is_no_open(Some("0")));
        assert!(!is_no_open(Some("false")));
        assert!(!is_no_open(None));
    }
}
