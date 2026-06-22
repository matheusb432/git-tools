//! Pending-diff handoff from CLI argv to the frontend. The Rust side queues
//! `diff://` refs (cold-start + single-instance forwards); the frontend drains
//! them on mount and also live-listens for `open-diff`, so neither path races.
use std::sync::Mutex;

/// The first `diff://…` argument in an argv, if any. Ignores argv[0] and flags.
pub fn diff_ref_from_argv(argv: &[String]) -> Option<String> {
    argv.iter().find(|a| a.starts_with("diff://")).cloned()
}

/// Managed Tauri state: diff refs awaiting the frontend.
#[derive(Default)]
pub struct PendingDiffs(pub Mutex<Vec<String>>);

impl PendingDiffs {
    pub fn push(&self, diff_ref: String) {
        self.0.lock().expect("pending-diffs lock").push(diff_ref);
    }

    /// Take and clear everything queued so far.
    pub fn drain(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().expect("pending-diffs lock"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_first_diff_url_ignoring_argv0_and_flags() {
        let argv = vec![
            "/usr/bin/gtl-viewer".to_string(),
            "--flag".to_string(),
            "diff://abc/def".to_string(),
            "diff://second/one".to_string(),
        ];
        assert_eq!(diff_ref_from_argv(&argv), Some("diff://abc/def".to_string()));
    }

    #[test]
    fn none_when_no_diff_url_present() {
        assert_eq!(diff_ref_from_argv(&["gtl-viewer".to_string()]), None);
    }

    #[test]
    fn queue_pushes_and_drains_fifo_then_empties() {
        let q = PendingDiffs::default();
        q.push("diff://a/1".into());
        q.push("diff://b/2".into());
        assert_eq!(q.drain(), vec!["diff://a/1".to_string(), "diff://b/2".to_string()]);
        assert!(q.drain().is_empty());
    }
}
