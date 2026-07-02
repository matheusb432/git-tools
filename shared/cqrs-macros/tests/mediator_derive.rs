//! Proves `#[derive(Mediator)]` dispatches correctly end-to-end: two fake requests, each routed
//! to the field that declares `#[handles(...)]` for it.

use std::{convert::Infallible, sync::Mutex};

use cqrs::{Behavior, Mediator, Next, Request, RequestHandler};

struct Ping;
impl Request for Ping {
    type Response = &'static str;
    type Error = Infallible;
    type Pipeline = ();
}

#[derive(Clone)]
struct PingHandler;
impl RequestHandler<Ping> for PingHandler {
    async fn handle(&self, _req: Ping) -> Result<&'static str, Infallible> {
        Ok("pong")
    }
}

struct Double(i32);
impl Request for Double {
    type Response = i32;
    type Error = Infallible;
    type Pipeline = ();
}

#[derive(Clone)]
struct DoubleHandler;
impl RequestHandler<Double> for DoubleHandler {
    async fn handle(&self, req: Double) -> Result<i32, Infallible> {
        Ok(req.0 * 2)
    }
}

#[derive(Clone, Mediator)]
struct TestMediator {
    #[handles(Ping)]
    ping: PingHandler,
    #[handles(Double)]
    double: DoubleHandler,
}

#[tokio::test]
async fn dispatches_to_the_field_registered_for_the_request_type() {
    let mediator = TestMediator {
        ping: PingHandler,
        double: DoubleHandler,
    };

    assert_eq!(mediator.handle(Ping).await.unwrap(), "pong");
    assert_eq!(mediator.handle(Double(21)).await.unwrap(), 42);
}

static ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

#[derive(Default)]
struct Outer;
impl<R: cqrs::Request + Send> Behavior<R> for Outer {
    async fn handle(&self, req: R, next: impl Next<R>) -> Result<R::Response, R::Error> {
        ORDER.lock().unwrap().push("outer");
        let out = next.run(req).await;
        ORDER.lock().unwrap().push("outer-done");
        out
    }
}

#[derive(Default)]
struct Inner;
impl<R: cqrs::Request + Send> Behavior<R> for Inner {
    async fn handle(&self, req: R, next: impl Next<R>) -> Result<R::Response, R::Error> {
        ORDER.lock().unwrap().push("inner");
        let out = next.run(req).await;
        ORDER.lock().unwrap().push("inner-done");
        out
    }
}

#[derive(Clone, Mediator)]
#[with(Outer, Inner)]
struct GlobalMediator {
    #[handles(Ping)]
    ping: PingHandler,
}

#[tokio::test]
async fn global_with_behaviors_wrap_every_dispatch_outermost_first() {
    ORDER.lock().unwrap().clear();
    let mediator = GlobalMediator { ping: PingHandler };
    assert_eq!(mediator.handle(Ping).await.unwrap(), "pong");
    assert_eq!(
        *ORDER.lock().unwrap(),
        vec!["outer", "inner", "inner-done", "outer-done"]
    );
}

// Dedicated static: `ORDER` above is already owned by the test above it, and tests in this
// binary run on parallel threads, so sharing one Mutex<Vec<_>> across two tests would race.
static E2E_ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

#[derive(Default)]
struct E2eOuter;
impl<R: cqrs::Request + Send> Behavior<R> for E2eOuter {
    async fn handle(&self, req: R, next: impl Next<R>) -> Result<R::Response, R::Error> {
        E2E_ORDER.lock().unwrap().push("outer");
        let out = next.run(req).await;
        E2E_ORDER.lock().unwrap().push("outer-done");
        out
    }
}

#[derive(Default)]
struct E2eInner;
impl<R: cqrs::Request + Send> Behavior<R> for E2eInner {
    async fn handle(&self, req: R, next: impl Next<R>) -> Result<R::Response, R::Error> {
        E2E_ORDER.lock().unwrap().push("inner");
        let out = next.run(req).await;
        E2E_ORDER.lock().unwrap().push("inner-done");
        out
    }
}

#[derive(cqrs::Request)]
#[request(response = &'static str, error = Infallible, with(E2eInner))]
struct E2ePing;

#[derive(Clone)]
struct E2ePingHandler;
impl RequestHandler<E2ePing> for E2ePingHandler {
    async fn handle(&self, _req: E2ePing) -> Result<&'static str, Infallible> {
        Ok("pong")
    }
}

#[derive(Clone, Mediator)]
#[with(E2eOuter)]
struct E2eMediator {
    #[handles(E2ePing)]
    ping: E2ePingHandler,
}

/// Proves the derive(Request)-declared request-level pipeline and the derive(Mediator)-declared
/// global pipeline compose in the documented order: mediator global wraps the request's own.
#[tokio::test]
async fn request_level_with_composes_inside_the_mediators_global_with() {
    E2E_ORDER.lock().unwrap().clear();
    let mediator = E2eMediator {
        ping: E2ePingHandler,
    };
    assert_eq!(mediator.handle(E2ePing).await.unwrap(), "pong");
    assert_eq!(
        *E2E_ORDER.lock().unwrap(),
        vec!["outer", "inner", "inner-done", "outer-done"]
    );
}
