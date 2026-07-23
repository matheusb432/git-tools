//! The content-addressed diff store: repo identity, content-hash addressing,
//! sidecar metadata, atomic placement, and range lookup. Formerly the
//! standalone `gtl-store` crate; see ADR-0002 / the GTL-0016 design spec.

pub mod id;
pub mod layout;
pub mod meta;

pub use id::{content_hash, repo_id};
pub use layout::{Placed, list_history_with_hash, lookup_by_range, place};
pub use meta::{DiffKind, RENDERER_VERSION, Sidecar};
