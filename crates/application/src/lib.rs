//! Application core (Ports & Adapters "inside"): ports plus vertical slices.
//!
//! Each feature operation owns its request and one public request-first
//! `#[cqrsy::handler(command|query)]` `execute` function. Callers import the
//! operation module and invoke `operation::execute(...)` directly.
//! This crate depends on the inward-facing domain and operation crates plus the
//! app-agnostic `gtl-recipe` DTO shared with process adapters.

pub mod branches;
pub mod diffs;
pub mod discovery;
pub mod history;
pub mod live_views;
pub mod managed;
pub mod ports;
pub mod push_subrepos;
pub mod recipes;
pub mod repository_sync;
pub mod settings;
pub mod shared;
pub mod squash_local;
pub mod tags;
pub mod viewer;
pub mod worktrees;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
