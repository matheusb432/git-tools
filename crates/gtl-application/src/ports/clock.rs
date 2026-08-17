use gtl_models::timestamps::{MachineTimestamp, TimestampError};

/// A source of the current machine time.
pub trait Clock: Clone + Send + Sync + 'static {
    /// Returns the current offset-qualified machine timestamp.
    fn now(&self) -> Result<MachineTimestamp, TimestampError>;
}
