//! The push-ledger seam's shipped adapter: no storage, no-op on every call.
//! A future effort replaces this with a real backing store (base spec Non-Goal).

use gtl_application::ports::{LedgerEntry, PushLedger};

#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpPushLedger;

impl PushLedger for NoOpPushLedger {
    async fn record(&self, _repo_name: &str, _ahead: usize, _checked_at: &str) {}
    async fn last_known(&self, _repo_name: &str) -> Option<LedgerEntry> {
        None
    }
    async fn refresh(&self) {}
}
