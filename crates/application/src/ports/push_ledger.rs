use std::future::Future;

/// One recorded push-ledger fact: the ahead-count observed and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    pub ahead: usize,
    pub checked_at: String,
}

/// The push-ledger seam. No real storage yet - `record`/`refresh` are no-ops and
/// `last_known` always answers `None` behind the shipped adapter; a future effort
/// gives this a real backing store and teaches `push_all` to consult `last_known`
/// to skip already-synced repos without a network round trip.
pub trait PushLedger: Clone + Send + Sync + 'static {
    fn record(
        &self,
        repo_name: &str,
        ahead: usize,
        checked_at: &str,
    ) -> impl Future<Output = ()> + Send;

    fn last_known(&self, repo_name: &str) -> impl Future<Output = Option<LedgerEntry>> + Send;

    /// Periodic refresh hook the daemon's background worker calls.
    fn refresh(&self) -> impl Future<Output = ()> + Send;
}
