//! Branch transition application operations.

pub mod apply_prune;
pub mod apply_rebase;
pub mod apply_revert;
pub mod apply_switch;
mod logic;
pub mod plan_prune;
pub mod plan_rebase;
pub mod plan_revert;
pub mod plan_switch;

pub use logic::branch_recovery::BranchRecovery;
