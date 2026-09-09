use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// Shared cancellation for CPU work, including Tree-sitter parsing.
#[derive(Clone, Debug, Default)]
pub struct ParseCancellation(Arc<AtomicUsize>);

impl ParseCancellation {
    pub fn cancel(&self) {
        self.0.store(1, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed) != 0
    }

    #[cfg(feature = "syntax")]
    pub(crate) fn flag(&self) -> &AtomicUsize {
        &self.0
    }
}
