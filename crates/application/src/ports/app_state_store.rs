/// Provides bounded mutable access to one initialized app-state connection.
///
/// Implementations must ensure that every clone shares the same initialized
/// backing connection.
pub trait AppStateStore: Clone + Send + Sync + 'static {
    /// Locks the process-owned connection for one synchronous application operation.
    fn connection_lock(
        &self,
    ) -> anyhow::Result<impl std::ops::DerefMut<Target = rusqlite::Connection> + '_>;
}
