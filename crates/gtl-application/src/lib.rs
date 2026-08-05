//! Application core (Ports & Adapters "inside"): ports plus vertical slices.
//!
//! Each feature operation owns its request and one public request-first
//! `#[cqrsy::command]` or `#[cqrsy::query]` `execute` function. cqrsy validates
//! its signature at compile time and generates no dispatch runtime. Callers
//! import the operation module and invoke `operation::execute(...)` directly.
//! This crate depends on the inward-facing models and operation crates plus the
//! app-agnostic recipe DTOs from `contracts` shared with process adapters.

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
pub mod tags;
pub mod viewer;
pub mod worktrees;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
