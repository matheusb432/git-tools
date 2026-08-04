//! The `push_subrepos` feature slice behind recursive `push -r`: discover every
//! git repo under a root, resolve each one's push destination from *local* refs
//! only (no fetch), then push the pushable ones.
//!
//! [`plan`] returns the structured targets that the process root presents and
//! confirms; [`apply`] pushes a confirmed plan. A repo with no upstream - or a
//! detached HEAD - carries a [`Dest::Skip`](gtl_models::managed::push_subrepos::Dest)
//! reason instead of a push target, so an un-pushable repo is unrepresentable as
//! a push and is reported, never silently dropped. A branch already synced with
//! its upstream (the local `@{u}..HEAD` count is `0`, the same check `gtl status
//! --all` reports) becomes a `Dest::Synced`, so synced repos never reach the
//! network.

pub mod apply;
pub mod plan;
