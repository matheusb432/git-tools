//! `Pipeline` — statically-composed behavior chains, plus the `dispatch` entry point.

use std::future::Future;

use crate::{Behavior, Next, Request, RequestHandler};

/// A statically-composed chain of [`Behavior`]s.
///
/// Implemented for `()` (run `next` directly) and `(B, Rest)` (B wraps Rest). Chains are
/// nested pairs of zero-sized behavior types, so a composed pipeline is itself zero-sized
/// and monomorphizes to the hand-written nesting.
pub trait Pipeline<R: Request>: Send + Sync {
    /// Runs `req` through every behavior in the chain, innermost being `next`.
    fn run(
        &self,
        req: R,
        next: impl Next<R>,
    ) -> impl Future<Output = Result<R::Response, R::Error>> + Send;
}

impl<R: Request + Send> Pipeline<R> for () {
    async fn run(&self, req: R, next: impl Next<R>) -> Result<R::Response, R::Error> {
        next.run(req).await
    }
}

impl<R, B, Rest> Pipeline<R> for (B, Rest)
where
    R: Request + Send,
    B: Behavior<R>,
    Rest: Pipeline<R>,
{
    async fn run(&self, req: R, next: impl Next<R>) -> Result<R::Response, R::Error> {
        self.0
            .handle(
                req,
                Chained {
                    rest: &self.1,
                    next,
                },
            )
            .await
    }
}

/// `Next` that runs the remaining chain segment before the innermost `next`.
struct Chained<'a, Rest, N> {
    rest: &'a Rest,
    next: N,
}

impl<R, Rest, N> Next<R> for Chained<'_, Rest, N>
where
    R: Request + Send,
    Rest: Pipeline<R>,
    N: Next<R>,
{
    async fn run(self, req: R) -> Result<R::Response, R::Error> {
        self.rest.run(req, self.next).await
    }
}

/// `Next` that runs a request's own pipeline, then the handler.
struct PipelinedHandler<'a, P, H> {
    pipeline: &'a P,
    handler: &'a H,
}

impl<R, P, H> Next<R> for PipelinedHandler<'_, P, H>
where
    R: Request + Send,
    P: Pipeline<R>,
    H: RequestHandler<R>,
{
    async fn run(self, req: R) -> Result<R::Response, R::Error> {
        self.pipeline
            .run(
                req,
                HandlerNext {
                    handler: self.handler,
                },
            )
            .await
    }
}

/// `Next` that terminates a pipeline by invoking the handler.
struct HandlerNext<'a, H> {
    handler: &'a H,
}

impl<R, H> Next<R> for HandlerNext<'_, H>
where
    R: Request + Send,
    H: RequestHandler<R>,
{
    async fn run(self, req: R) -> Result<R::Response, R::Error> {
        self.handler.handle(req).await
    }
}

/// Dispatches `req` through `global`, then `R::Pipeline`, then `handler`.
///
/// This is the mediator entry point: `#[derive(Mediator)]`-generated `RequestHandler` impls
/// call it. Call it directly only for bespoke wiring outside a mediator.
///
/// # Errors
///
/// Returns whatever `R::Error` the handler or a short-circuiting behavior produces.
pub async fn dispatch<R, G, H>(global: &G, handler: &H, req: R) -> Result<R::Response, R::Error>
where
    R: Request + Send,
    G: Pipeline<R>,
    H: RequestHandler<R>,
{
    let request_pipeline = <R::Pipeline as Default>::default();
    global
        .run(
            req,
            PipelinedHandler {
                pipeline: &request_pipeline,
                handler,
            },
        )
        .await
}
