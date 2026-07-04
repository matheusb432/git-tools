//! Wire-format DTOs for the daemon boundary. Intentionally free of any
//! `domain`/`application` dependency — process roots own the mapping
//! (template pattern; enforced by the check-deps lint).

pub mod diffs;
pub mod envelope;
pub mod managed;
