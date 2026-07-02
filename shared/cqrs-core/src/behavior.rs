//! `Behavior` and `Next` — the pipeline middleware contracts.

use std::future::Future;

use crate::Request;

/// One-shot continuation handed to a [`Behavior`]: the rest of the pipeline, ending at the
/// handler (MediatR's `next` delegate).
pub trait Next<R: Request>: Send {
    /// Runs the remainder of the pipeline with `req`.
    fn run(self, req: R) -> impl Future<Output = Result<R::Response, R::Error>> + Send;
}

/// Cross-cutting middleware around one dispatch.
///
/// A behavior may call `next.run(req)` exactly once (pass-through, e.g. logging/timing), or
/// return early without calling it (short-circuit). Behaviors are attached statically — see
/// `Request::Pipeline` and the `Mediator` derive's `#[with(...)]` — and must be zero-sized or
/// at least [`Default`], because generated code constructs them.
pub trait Behavior<R: Request>: Send + Sync {
    /// Processes `req`, delegating to `next` for the rest of the pipeline.
    fn handle(
        &self,
        req: R,
        next: impl Next<R>,
    ) -> impl Future<Output = Result<R::Response, R::Error>> + Send;
}
