//! Environment-variable parsing helpers shared by the process roots.

use std::env::VarError;

use anyhow::{Context as _, anyhow};

/// Parses environment variable `key` into `T`, falling back to `default` when it is unset.
///
/// A present-but-malformed value is a hard error (parse failure or non-UTF-8 bytes); only a
/// genuinely absent variable falls back to `default`. This keeps a misconfigured deployment
/// failing fast at boot with a clear message rather than silently using a default.
///
/// # Errors
///
/// Returns an error if the variable is present but cannot be parsed into `T`, or holds non-UTF-8
/// bytes.
pub fn parse_env_or<T>(key: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match std::env::var(key) {
        Ok(raw) => raw
            .parse()
            .with_context(|| format!("{key} is not a valid value: {raw:?}")),
        Err(VarError::NotPresent) => Ok(default),
        Err(VarError::NotUnicode(raw)) => {
            Err(anyhow!("{key} is not valid UTF-8: {}", raw.display()))
        }
    }
}

/// Reads an optional environment variable, distinguishing absent from malformed.
///
/// # Errors
/// Returns an error if the variable holds non-UTF-8 bytes.
pub fn optional_env(key: &str) -> anyhow::Result<Option<String>> {
    match std::env::var(key) {
        Ok(raw) => Ok(Some(raw)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(raw)) => {
            Err(anyhow!("{key} is not valid UTF-8: {}", raw.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::parse_env_or;

    /// All scenarios live in one test: mutating the process-global environment from multiple
    /// concurrent tests would race at the libc level, so the env cases run sequentially here.
    #[test]
    fn parse_env_or_resolves_absent_valid_and_malformed() {
        let key = "BOOTSTRAP_TEST_PARSE_ENV_OR";

        // Absent → default.
        unsafe { std::env::remove_var(key) };
        assert_eq!(parse_env_or::<u32>(key, 10).unwrap(), 10);

        // Present + valid → parsed.
        unsafe { std::env::set_var(key, "42") };
        assert_eq!(parse_env_or::<u32>(key, 10).unwrap(), 42);

        // Present + malformed → error (never silently falls back to the default).
        unsafe { std::env::set_var(key, "not-a-socket") };
        assert!(parse_env_or::<SocketAddr>(key, "0.0.0.0:8080".parse().unwrap()).is_err());

        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn optional_env_distinguishes_absent_from_present() {
        let key = "BOOTSTRAP_TEST_OPTIONAL_ENV";

        unsafe { std::env::remove_var(key) };
        assert_eq!(super::optional_env(key).unwrap(), None);

        unsafe { std::env::set_var(key, "value") };
        assert_eq!(super::optional_env(key).unwrap(), Some("value".to_string()));

        unsafe { std::env::remove_var(key) };
    }
}
