/// A source of the current time as an ISO-8601 string.
pub trait Clock: Clone + Send + Sync + 'static {
    /// The current instant as a strict ISO-8601 timestamp string.
    fn now_iso(&self) -> String;
}
