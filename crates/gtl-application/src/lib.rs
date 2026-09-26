//! Application core (Ports & Adapters "inside"): ports plus vertical slices.
//!
//! Each feature operation owns its request and one public request-first
//! `#[cqrsy::command]` or `#[cqrsy::query]` `execute` function. cqrsy validates
//! its signature at compile time and generates no dispatch runtime. Callers
//! import the operation module and invoke `operation::execute(...)` directly.
//! This crate depends on inward-facing models plus typed wire values shared with process adapters.

pub mod diffs;
pub mod history;
pub mod ports;
pub mod projects;
pub mod recipes;
pub mod repositories;
pub mod settings;
pub mod shared;
pub mod tags;
pub mod viewer;

#[cfg(any(test, feature = "testing"))]
pub mod utils;
