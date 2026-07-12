//! Shared daemon state and the route table.
//!
//! [`DaemonMediator`] owns one generated handler per operation and implements
//! `Sender<R>` through a declarative `cqrsy::mediator!` mapping. [`AppState`]
//! bundles the mediator with cross-cutting [`Shared`] state; two `FromRef`
//! implementations bridge it to the extractors each endpoint asks for.

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
use tokio::sync::watch;

use crate::{endpoints, lifecycle::ExeIdentity};

/// Owns the operation dependencies assembled by the daemon process root.
pub struct DaemonDependencies<S, A, R, C, RS, ML, PL, P, AS> {
    pub source: S,
    pub artifacts: A,
    pub renderer: R,
    pub clock: C,
    pub remote: RS,
    pub manifest: ML,
    pub ledger: PL,
    pub probe: P,
    pub app_state: AS,
}

cqrsy::mediator! {
    /// Dispatches every daemon operation through its generated handler.
    #[derive(Clone)]
    pub struct DaemonMediator<S, A, R, C, RS, ML, PL, P, AS>
        from DaemonDependencies<S, A, R, C, RS, ML, PL, P, AS>
    where
        S: DiffSource + cqrsy::Handle,
        A: ArtifactStore + cqrsy::Handle,
        R: HtmlRenderer + cqrsy::Handle,
        C: Clock + cqrsy::Handle,
        RS: RemoteSync + cqrsy::Handle,
        ML: ManagedManifest + cqrsy::Handle,
        PL: PushLedger + cqrsy::Handle,
        P: RepoProbe + cqrsy::Handle,
        AS: AppStateStore + cqrsy::Handle,
    {
        state {}
        handlers {
            RenderDiff => render_diff: RenderDiffHandler<S, A, R, C> = |state| RenderDiffHandler {
                source: state.source.clone(),
                store: state.artifacts.clone(),
                renderer: state.renderer.clone(),
                clock: state.clock.clone(),
            },
            RenderMergeDiff => render_merge_diff: RenderMergeDiffHandler<S, A, R, C> = |state| RenderMergeDiffHandler {
                source: state.source.clone(),
                store: state.artifacts.clone(),
                renderer: state.renderer.clone(),
                clock: state.clock.clone(),
            },
            RenderSquashPreview => render_squash_preview: RenderSquashPreviewHandler<S, A, R, C> = |state| RenderSquashPreviewHandler {
                source: state.source.clone(),
                store: state.artifacts.clone(),
                renderer: state.renderer.clone(),
                clock: state.clock.clone(),
            },
            RenderDiffSubrepos => render_diff_subrepos: RenderDiffSubreposHandler<S, A, R, C> = |state| RenderDiffSubreposHandler {
                source: state.source.clone(),
                store: state.artifacts.clone(),
                renderer: state.renderer.clone(),
                clock: state.clock.clone(),
            },
            RenderDiffAll => render_diff_all: RenderDiffAllHandler<S, A, R, C> = |state| RenderDiffAllHandler {
                source: state.source.clone(),
                store: state.artifacts.clone(),
                renderer: state.renderer.clone(),
                clock: state.clock.clone(),
            },
            PushAll => push_all: PushAllHandler<RS, ML, PL, C> = |state| PushAllHandler {
                remote: state.remote.clone(),
                manifest: state.manifest.clone(),
                ledger: state.ledger.clone(),
                clock: state.clock.clone(),
            },
            PullAll => pull_all: PullAllHandler<RS, ML> = |state| PullAllHandler {
                remote: state.remote.clone(),
                manifest: state.manifest.clone(),
            },
            SaveLiveView => save_live_view: SaveLiveViewHandler<P, AS, C> = |state| SaveLiveViewHandler {
                probe: state.probe.clone(),
                store: state.app_state.clone(),
                clock: state.clock.clone(),
            },
        }
    }
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
