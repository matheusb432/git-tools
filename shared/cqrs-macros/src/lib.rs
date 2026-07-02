//! Two derives that remove `cqrs` boilerplate: `#[derive(Request)]` generates a request's
//! `Request` impl (response/error types plus its own `with(...)` behavior pipeline) from a
//! `#[request(...)]` attribute, and `#[derive(Mediator)]` generates one zero-cost
//! `RequestHandler<R>` forwarding impl per `#[handles(R)]`-annotated field, routing each call
//! through `::cqrs::dispatch` and the struct's own `#[with(...)]` global pipeline. See the
//! `cqrs` crate for the runtime traits both derives dispatch to.

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod mediator;
mod request;

#[cfg(test)]
mod doc_expansion_tests;

/// Derives the **mediator via facade** pattern: the annotated struct becomes the application's
/// single dispatch surface, implementing `RequestHandler<R>` once for every request `R` named
/// in a field's `#[handles(R)]` attribute.
///
/// The struct itself is nothing but a bag of handler fields — one field per operation, wired by
/// the composition root. The derive adds no business logic: for each `#[handles(R)]` field it
/// emits an impl that routes the request through `::cqrs::dispatch` — the struct's global
/// `#[with(...)]` pipeline (or `()` when absent), then the request's own `Request::Pipeline`,
/// then `self.<field>`. Callers never name a field; they bound themselves to
/// `RequestHandler<TheirRequest>` and call `.handle(request)`, and the compiler picks the field
/// from the request's type — monomorphized dispatch, no `dyn`, no boxing, no runtime registry.
///
/// # The `#[handles(...)]` field attribute
///
/// - Takes exactly one path: the `Request` type this field serves, e.g. `#[handles(GetTodos)]`.
/// - The field's type must itself implement `RequestHandler` for that request, or the generated
///   forwarding impl fails to compile.
/// - A field with no `#[handles(...)]` attribute is plain data; the derive skips it.
/// - Generic parameters and `where` clauses on the struct are preserved on every generated impl.
///
/// # Examples
///
/// ```
/// use std::convert::Infallible;
///
/// use cqrs::{Mediator, Request, RequestHandler};
///
/// struct Ping;
/// impl Request for Ping {
///     type Response = &'static str;
///     type Error = Infallible;
///     type Pipeline = ();
/// }
///
/// #[derive(Clone)]
/// struct PingHandler;
/// impl RequestHandler<Ping> for PingHandler {
///     async fn handle(&self, _req: Ping) -> Result<&'static str, Infallible> {
///         Ok("pong")
///     }
/// }
///
/// /// One handler field per operation — the app's single dispatch facade.
/// #[derive(Clone, Mediator)]
/// struct AppMediator {
///     #[handles(Ping)]
///     ping: PingHandler,
/// }
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let mediator = AppMediator { ping: PingHandler };
/// // Callers depend on `RequestHandler<Ping>` alone — never on the `ping` field.
/// assert_eq!(mediator.handle(Ping).await.unwrap(), "pong");
/// # }
/// ```
///
/// The `#[handles(Ping)]` attribute above maps to exactly this impl (one per annotated field,
/// nothing else) — `AppMediator` has no `#[with(...)]`, so the global pipeline is the empty `()`:
// ! Pinned against drift by `doc_expansion_tests::mediator_doc_expansion_matches_real_expansion`.
/// ```ignore
/// impl ::cqrs::RequestHandler<Ping> for AppMediator {
///     async fn handle(
///         &self,
///         req: Ping,
///     ) -> ::core::result::Result<<Ping as ::cqrs::Request>::Response, <Ping as ::cqrs::Request>::Error,> {
///         let global: () = ::core::default::Default::default();
///         ::cqrs::dispatch(&global, &self.ping, req).await
///     }
/// }
/// ```
///
/// # Global behaviors — `#[with(...)]`
///
/// An optional struct-level `#[with(B1, B2, …)]` attribute declares the mediator's global
/// behavior pipeline — cross-cutting `cqrs::Behavior`s (logging, timing, …) that wrap *every*
/// dispatch through this mediator, regardless of which request is being handled:
///
/// ```ignore
/// #[derive(Clone, Mediator)]
/// #[with(Logged, Timed)]
/// struct AppMediator {
///     #[handles(Ping)]
///     ping: PingHandler,
/// }
/// ```
///
/// Each generated body constructs the declared behaviors as a nested tuple via `Default` (so
/// every behavior in `#[with(...)]` must implement `Default`) and routes the call through
/// `::cqrs::dispatch`. Without a `#[with(...)]` attribute, the global pipeline is `()` — no
/// behaviors, behaviorally equivalent to a plain `self.<field>.handle(req).await` (the empty
/// chains reduce to the plain handler call after inlining).
///
/// Behaviors compose in a fixed order, outermost to innermost:
///
/// 1. The mediator's own `#[with(B1, B2, …)]`, left-to-right (`B1` sees the request first).
/// 2. The request's own `Request::Pipeline`.
/// 3. The handler.
///
/// Pipelines run **only** on mediator dispatch (i.e. through a `#[derive(Mediator)]`-generated
/// `RequestHandler` impl) — calling a handler's `handle` method directly bypasses both the
/// global and the request's own pipeline.
///
/// # Errors (compile-time)
///
/// - Applied to anything other than a named-field struct.
/// - A `#[handles(...)]` attribute that doesn't contain exactly one path.
/// - More than one `#[handles(...)]` attribute on the same field.
/// - More than one `#[with(...)]` attribute on the struct.
/// - A `#[with(...)]` attribute with zero behavior paths.
#[proc_macro_derive(Mediator, attributes(handles, with))]
pub fn derive_mediator(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    mediator::expand_mediator(&input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Derives `cqrs::Request` for a request struct from its `#[request(...)]` attribute, so the
/// contract (response type, error type, and behavior pipeline) is declared once, next to the
/// struct, instead of a hand-written `impl Request` block.
///
/// # The `#[request(...)]` attribute
///
/// - `response = <Type>` — required. The type `Request::Response` resolves to.
/// - `error = <Type>` — required. The type `Request::Error` resolves to.
/// - `with(B1, B2, …)` — optional. This request's own behavior pipeline, distinct from any
///   mediator-level `#[with(...)]` — see [`derive_mediator`]'s "Global behaviors" section for how
///   the two compose. Declaring `with(...)` with zero paths is rejected.
/// - At most one `#[request(...)]` attribute is allowed on the struct.
///
/// # Examples
///
/// ```
/// use std::convert::Infallible;
///
/// use cqrs::{Mediator, Request, RequestHandler};
///
/// #[derive(Request)]
/// #[request(response = &'static str, error = Infallible)]
/// struct Ping;
///
/// #[derive(Clone)]
/// struct PingHandler;
/// impl RequestHandler<Ping> for PingHandler {
///     async fn handle(&self, _req: Ping) -> Result<&'static str, Infallible> {
///         Ok("pong")
///     }
/// }
///
/// #[derive(Clone, Mediator)]
/// struct AppMediator {
///     #[handles(Ping)]
///     ping: PingHandler,
/// }
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let mediator = AppMediator { ping: PingHandler };
/// assert_eq!(mediator.handle(Ping).await.unwrap(), "pong");
/// # }
/// ```
///
/// The `#[request(response = &'static str, error = Infallible)]` attribute above maps to exactly
/// this impl — no `with(...)`, so the request's own pipeline is the empty `()`:
// ! Pinned against drift by `doc_expansion_tests::request_doc_expansion_matches_real_expansion`.
/// ```ignore
/// impl ::cqrs::Request for Ping {
///     type Response = &'static str;
///     type Error = Infallible;
///     type Pipeline = ();
/// }
/// ```
///
/// With `#[request(response = (), error = PingError, with(Logged, Timed))]`, `Pipeline` becomes
/// the nested tuple `(Logged, (Timed, ()))` instead of `()` — see [`derive_mediator`] for how a
/// request's own pipeline composes with a mediator's global one.
///
/// # Errors (compile-time)
///
/// - No `#[request(...)]` attribute on the struct at all.
/// - `#[request(...)]` missing `response = ...`.
/// - `#[request(...)]` missing `error = ...`.
/// - `with(...)` present with zero behavior paths.
/// - More than one `#[request(...)]` attribute on the struct.
#[proc_macro_derive(Request, attributes(request))]
pub fn derive_request(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    request::expand_request(&input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
