//! `Request` and `RequestHandler` — the core CQRS dispatch contracts.

use std::future::Future;

use crate::Pipeline;

/// A command or query: owns its response and error contract.
///
/// The request type carries both the success type ([`Request::Response`]) and the failure type
/// ([`Request::Error`]), so a handler is keyed purely by which request it serves.
pub trait Request: Sized {
    /// The success value produced by handling this request.
    type Response;
    /// The typed failure this request can produce.
    type Error: std::error::Error + Send + Sync + 'static;
    /// The behavior chain run around this request's handler on mediator dispatch.
    ///
    /// `()` means "no behaviors". Composed as nested pairs of zero-sized behavior types
    /// (e.g. `(Logged, (Timed, ()))`), constructed via [`Default`] by generated dispatch code.
    type Pipeline: Pipeline<Self> + Default;
}

/// Handles exactly one [`Request`] type.
///
/// Monomorphized per request — no `dyn`, no `async-trait`, no boxing. The explicit RPITIT
/// `-> impl Future + Send` keeps handler futures usable on tokio's multi-threaded runtime; an
/// `async fn handle` body satisfies it whenever the types held across `.await` are `Send`. A
/// synchronous body (no `.await`) satisfies the trait just as well — no separate sync handler
/// trait is needed.
pub trait RequestHandler<R: Request>: Send + Sync {
    /// Processes `req` and returns its [`Request::Response`] or [`Request::Error`].
    fn handle(&self, req: R) -> impl Future<Output = Result<R::Response, R::Error>> + Send;
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;

    struct Triple;
    impl Request for i64 {
        type Response = i64;
        type Error = Infallible;
        type Pipeline = ();
    }
    impl RequestHandler<i64> for Triple {
        async fn handle(&self, req: i64) -> Result<i64, Infallible> {
            Ok(req * 3)
        }
    }

    #[tokio::test]
    async fn request_handler_returns_response() {
        assert_eq!(Triple.handle(14).await.unwrap(), 42);
    }
}
