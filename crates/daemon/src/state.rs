//! Shared daemon state and the route table.
//!
//! [`DaemonMediator`] owns one handler instance per operation and, via
//! `#[derive(cqrsy::Mediator)]`, implements `Sender<R>` for every `#[handles(R)]` field —
//! dispatch by type, monomorphized, no `dyn`. [`AppState`] bundles the mediator with the
//! cross-cutting [`Shared`] state; the two hand-written `FromRef` impls bridge `AppState` to the
//! extractors each endpoint asks for.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use application::{
    diffs::{
        render_diff::{RenderDiff, RenderDiffHandler},
        render_diff_all::{RenderDiffAll, RenderDiffAllHandler},
        render_diff_subrepos::{RenderDiffSubrepos, RenderDiffSubreposHandler},
        render_merge_diff::{RenderMergeDiff, RenderMergeDiffHandler},
        render_squash_preview::{RenderSquashPreview, RenderSquashPreviewHandler},
    },
    live_views::save::{SaveLiveView, SaveLiveViewHandler},
    managed::{
        pull_all::{PullAll, PullAllHandler},
        push_all::{PushAll, PushAllHandler},
    },
    ports::{
        AppStateStore, ArtifactStore, Clock, DiffSource, HtmlRenderer, ManagedManifest, PushLedger,
        RemoteSync, RepoProbe,
    },
};
use axum::{
    Router,
    extract::FromRef,
    routing::{get, post},
};
use cqrsy::Mediator;
use tokio::sync::watch;

use crate::{endpoints, lifecycle::ExeIdentity};

/// The daemon's dispatch facade — one handler field per operation.
///
/// `#[derive(cqrsy::Mediator)]` implements `Sender<RenderDiff>` for this struct, forwarding
/// to the `render_diff` field. Endpoints bound to `Sender<RenderDiff>` dispatch through the
/// facade without naming the field.
#[derive(Clone, Mediator)]
pub struct DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    R: HtmlRenderer + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
    RS: RemoteSync + Clone + Send + Sync + 'static,
    ML: ManagedManifest + Clone + Send + Sync + 'static,
    PL: PushLedger + Clone + Send + Sync + 'static,
    P: RepoProbe + Clone + Send + Sync + 'static,
    AS: AppStateStore + Clone + Send + Sync + 'static,
{
    /// Render handler for `POST /diffs/render`.
    #[handles(RenderDiff)]
    pub render_diff: RenderDiffHandler<S, A, R, C>,
    /// Render handler for `POST /diffs/merge`.
    #[handles(RenderMergeDiff)]
    pub render_merge_diff: RenderMergeDiffHandler<S, A, R, C>,
    /// Render handler for `POST /diffs/squash-preview`.
    #[handles(RenderSquashPreview)]
    pub render_squash_preview: RenderSquashPreviewHandler<S, A, R, C>,
    /// Render handler for `POST /diffs/subrepos`.
    #[handles(RenderDiffSubrepos)]
    pub render_diff_subrepos: RenderDiffSubreposHandler<S, A, R, C>,
    /// Render handler for `POST /diffs/all`.
    #[handles(RenderDiffAll)]
    pub render_diff_all: RenderDiffAllHandler<S, A, R, C>,
    /// Handler for `POST /managed/push-all`.
    #[handles(PushAll)]
    pub push_all: PushAllHandler<RS, ML, PL, C>,
    /// Handler for `POST /managed/pull-all`.
    #[handles(PullAll)]
    pub pull_all: PullAllHandler<RS, ML>,
    /// Handler for `POST /live-views/save`.
    #[handles(SaveLiveView)]
    pub save_live_view: SaveLiveViewHandler<P, AS, C>,
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
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Dependency bundle injected into handlers via axum's `State`/`FromRef` extractors.
#[derive(Clone)]
pub struct AppState<S, A, R, C, RS, ML, PL, P, AS>
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    R: HtmlRenderer + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
    RS: RemoteSync + Clone + Send + Sync + 'static,
    ML: ManagedManifest + Clone + Send + Sync + 'static,
    PL: PushLedger + Clone + Send + Sync + 'static,
    P: RepoProbe + Clone + Send + Sync + 'static,
    AS: AppStateStore + Clone + Send + Sync + 'static,
{
    pub mediator: DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>,
    pub shared: Arc<Shared>,
}

impl<S, A, R, C, RS, ML, PL, P, AS> FromRef<AppState<S, A, R, C, RS, ML, PL, P, AS>>
    for DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    R: HtmlRenderer + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
    RS: RemoteSync + Clone + Send + Sync + 'static,
    ML: ManagedManifest + Clone + Send + Sync + 'static,
    PL: PushLedger + Clone + Send + Sync + 'static,
    P: RepoProbe + Clone + Send + Sync + 'static,
    AS: AppStateStore + Clone + Send + Sync + 'static,
{
    fn from_ref(s: &AppState<S, A, R, C, RS, ML, PL, P, AS>) -> Self {
        s.mediator.clone()
    }
}

impl<S, A, R, C, RS, ML, PL, P, AS> FromRef<AppState<S, A, R, C, RS, ML, PL, P, AS>> for Arc<Shared>
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    R: HtmlRenderer + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
    RS: RemoteSync + Clone + Send + Sync + 'static,
    ML: ManagedManifest + Clone + Send + Sync + 'static,
    PL: PushLedger + Clone + Send + Sync + 'static,
    P: RepoProbe + Clone + Send + Sync + 'static,
    AS: AppStateStore + Clone + Send + Sync + 'static,
{
    fn from_ref(s: &AppState<S, A, R, C, RS, ML, PL, P, AS>) -> Self {
        s.shared.clone()
    }
}

/// Builds the fully-wired daemon router for a given [`AppState`].
///
/// The `/health` and `/shutdown` handlers extract only `State<Arc<Shared>>`, so they stay
/// non-generic; every `/diffs/*` route is monomorphized on the mediator type.
// The explicit `Router<AppState<..>>` binding below is inherent to a 9-type-param
// composition root (one pair per port); factoring it into an alias wouldn't reduce
// its real complexity, so this suppresses the noise rather than obscuring the type.
#[allow(clippy::type_complexity)]
pub fn router<S, A, R, C, RS, ML, PL, P, AS>(
    state: AppState<S, A, R, C, RS, ML, PL, P, AS>,
) -> Router
where
    S: DiffSource + Clone + Send + Sync + 'static,
    A: ArtifactStore + Clone + Send + Sync + 'static,
    R: HtmlRenderer + Clone + Send + Sync + 'static,
    C: Clock + Clone + Send + Sync + 'static,
    RS: RemoteSync + Clone + Send + Sync + 'static,
    ML: ManagedManifest + Clone + Send + Sync + 'static,
    PL: PushLedger + Clone + Send + Sync + 'static,
    P: RepoProbe + Clone + Send + Sync + 'static,
    AS: AppStateStore + Clone + Send + Sync + 'static,
{
    // Explicit `Router<AppState<..>>` fixes S up front so axum can resolve the `FromRef` bounds
    // while routes are chained.
    let routes: Router<AppState<S, A, R, C, RS, ML, PL, P, AS>> = Router::new()
        .route("/health", get(endpoints::health::handle))
        .route("/shutdown", post(endpoints::shutdown::handle))
        .route(
            "/diffs/render",
            post(endpoints::diffs::render::handle::<DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>>),
        )
        .route(
            "/diffs/merge",
            post(endpoints::diffs::merge::handle::<DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>>),
        )
        .route(
            "/diffs/squash-preview",
            post(
                endpoints::diffs::squash_preview::handle::<
                    DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>,
                >,
            ),
        )
        .route(
            "/diffs/subrepos",
            post(
                endpoints::diffs::subrepos::handle::<DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>>,
            ),
        )
        .route(
            "/diffs/all",
            post(endpoints::diffs::all::handle::<DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>>),
        )
        .route(
            "/managed/push-all",
            post(
                endpoints::managed::push_all::handle::<DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>>,
            ),
        )
        .route(
            "/managed/pull-all",
            post(
                endpoints::managed::pull_all::handle::<DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>>,
            ),
        )
        .route(
            "/live-views/save",
            post(
                endpoints::live_views::save::handle::<DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>>,
            ),
        );
    routes.with_state(state)
}
