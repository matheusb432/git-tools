//! The live-views feature: persisted repo views the app renders on demand.

pub mod list;
mod logic;
pub mod probe;
pub mod remove;
pub mod save;

pub use logic::persistence::LiveViewRecord;
