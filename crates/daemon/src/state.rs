//! Shared daemon state and the route table.
//!
//! [`DaemonMediator`] owns one handler instance per operation and, via
//! `#[derive(cqrs::Mediator)]`, implements `RequestHandler<R>` for every `#[handles(R)]` field —
//! dispatch by type, monomorphized, no `dyn`. [`AppState`] bundles the mediator with the
//! cross-cutting [`Shared`] state; the two hand-written `FromRef` impls bridge `AppState` to the
//! extractors each endpoint asks for.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use application::{
    diffs::render_diff::{RenderDiff, RenderDiffHandler},
    ports::{ArtifactStore, Clock, DiffSource, HtmlRenderer},
};
use axum::{
    Router,
    extract::FromRef,
    routing::{get, post},
};
use cqrs::Mediator;
use tokio::sync::watch;

use crate::{endpoints, lifecycle::ExeIdentity};

/// The daemon's dispatch facade — one handler field per operation.
///
/// `#[derive(cqrs::Mediator)]` implements `RequestHandler<RenderDiff>` for this struct, forwarding
/// to the `render_diff` field. Endpoints bound to `RequestHandler<RenderDiff>` dispatch through the
/// facade without naming the field.
#[derive(Clone, Mediator)]
pub struct DaemonMediator<S, A, R, C>
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    R: HtmlRenderer + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
{
    /// Render handler for `POST /diffs/render`.
    #[handles(RenderDiff)]
    pub render_diff: RenderDiffHandler<S, A, R, C>,
}

/// Cross-cutting daemon state: identity for the handshake, the shutdown
/// trigger, and the idle tracker (seconds granularity is plenty).
pub struct Shared {
    pub identity: ExeIdentity,
    pub version: &'static str,
    pub pid: u32,
    pub shutdown_tx: watch::Sender<bool>,
    pub last_activity_ms: AtomicU64,
}

impl Shared {
    /// Record that a request just arrived, resetting the idle countdown.
    pub fn touch(&self) {
        self.last_activity_ms.store(now_ms(), Ordering::Relaxed);
    }

    /// The last-activity instant, in epoch milliseconds.
    pub fn last_activity_ms(&self) -> u64 {
        self.last_activity_ms.load(Ordering::Relaxed)
    }
}

/// Wall-clock now in epoch milliseconds (saturating on the impossible pre-epoch case).
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Dependency bundle injected into handlers via axum's `State`/`FromRef` extractors.
#[derive(Clone)]
pub struct AppState<S, A, R, C>
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    R: HtmlRenderer + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
{
    pub mediator: DaemonMediator<S, A, R, C>,
    pub shared: Arc<Shared>,
}

impl<S, A, R, C> FromRef<AppState<S, A, R, C>> for DaemonMediator<S, A, R, C>
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    R: HtmlRenderer + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
{
    fn from_ref(s: &AppState<S, A, R, C>) -> Self {
        s.mediator.clone()
    }
}

impl<S, A, R, C> FromRef<AppState<S, A, R, C>> for Arc<Shared>
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    R: HtmlRenderer + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
{
    fn from_ref(s: &AppState<S, A, R, C>) -> Self {
        s.shared.clone()
    }
}

/// Builds the fully-wired daemon router for a given [`AppState`].
///
/// The `/health` and `/shutdown` handlers extract only `State<Arc<Shared>>`, so they stay
/// non-generic; only `/diffs/render` is monomorphized on the mediator type.
pub fn router<S, A, R, C>(state: AppState<S, A, R, C>) -> Router
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    R: HtmlRenderer + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
{
    // Explicit `Router<AppState<..>>` fixes S up front so axum can resolve the `FromRef` bounds
    // while routes are chained.
    let routes: Router<AppState<S, A, R, C>> = Router::new()
        .route("/health", get(endpoints::health::handle))
        .route("/shutdown", post(endpoints::shutdown::handle))
        .route(
            "/diffs/render",
            post(endpoints::diffs::render::handle::<DaemonMediator<S, A, R, C>>),
        );
    routes.with_state(state)
}
