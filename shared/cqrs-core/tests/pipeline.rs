// shared/cqrs-core/tests/pipeline.rs
//! Proves pipeline mechanics: order, short-circuit, error pass-through, empty chains.

use std::sync::{Arc, Mutex};

use cqrs_core::{Behavior, Next, Request, RequestHandler, dispatch};

#[derive(Debug)]
struct Ping;
impl Request for Ping {
    type Response = &'static str;
    type Error = PingError;
    type Pipeline = ();
}

#[derive(Debug, thiserror::Error)]
#[error("ping failed: {0}")]
struct PingError(&'static str);

struct PingHandler {
    log: Arc<Mutex<Vec<&'static str>>>,
    fail: bool,
}
impl RequestHandler<Ping> for PingHandler {
    async fn handle(&self, _req: Ping) -> Result<&'static str, PingError> {
        self.log.lock().unwrap().push("handler");
        if self.fail {
            Err(PingError("boom"))
        } else {
            Ok("pong")
        }
    }
}

/// Records entry before `next` and exit after — proves outer-in / inner-out ordering.
struct Recorder {
    name: &'static str,
    log: Arc<Mutex<Vec<&'static str>>>,
}
impl Behavior<Ping> for Recorder {
    async fn handle(&self, req: Ping, next: impl Next<Ping>) -> Result<&'static str, PingError> {
        self.log.lock().unwrap().push(self.name);
        let out = next.run(req).await;
        self.log.lock().unwrap().push(self.name);
        out
    }
}

/// Never calls `next` — proves short-circuiting.
struct ShortCircuit;
impl Behavior<Ping> for ShortCircuit {
    async fn handle(&self, _req: Ping, _next: impl Next<Ping>) -> Result<&'static str, PingError> {
        Ok("intercepted")
    }
}

fn recorder(name: &'static str, log: &Arc<Mutex<Vec<&'static str>>>) -> Recorder {
    Recorder {
        name,
        log: Arc::clone(log),
    }
}

#[tokio::test]
async fn empty_global_pipeline_just_calls_the_handler() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let handler = PingHandler {
        log: Arc::clone(&log),
        fail: false,
    };
    assert_eq!(dispatch(&(), &handler, Ping).await.unwrap(), "pong");
    assert_eq!(*log.lock().unwrap(), vec!["handler"]);
}

#[tokio::test]
async fn behaviors_run_outermost_first_and_unwind_in_reverse() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let handler = PingHandler {
        log: Arc::clone(&log),
        fail: false,
    };
    let global = (recorder("a", &log), (recorder("b", &log), ()));
    assert_eq!(dispatch(&global, &handler, Ping).await.unwrap(), "pong");
    assert_eq!(*log.lock().unwrap(), vec!["a", "b", "handler", "b", "a"]);
}

#[tokio::test]
async fn a_behavior_can_short_circuit_without_reaching_the_handler() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let handler = PingHandler {
        log: Arc::clone(&log),
        fail: false,
    };
    let global = (recorder("a", &log), (ShortCircuit, ()));
    assert_eq!(
        dispatch(&global, &handler, Ping).await.unwrap(),
        "intercepted"
    );
    assert_eq!(*log.lock().unwrap(), vec!["a", "a"]);
}

#[tokio::test]
async fn errors_pass_through_behaviors_unchanged() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let handler = PingHandler {
        log: Arc::clone(&log),
        fail: true,
    };
    let global = (recorder("a", &log), ());
    let err = dispatch(&global, &handler, Ping).await.unwrap_err();
    assert_eq!(err.0, "boom");
    assert_eq!(*log.lock().unwrap(), vec!["a", "handler", "a"]);
}

/// The whole dispatch future must be `Send` (axum/tokio requirement) — compile-time proof.
#[test]
fn dispatch_future_is_send() {
    fn assert_send<T: Send>(_: T) {}
    let log = Arc::new(Mutex::new(Vec::new()));
    let handler = PingHandler {
        log: Arc::clone(&log),
        fail: false,
    };
    let global = (recorder("a", &log), ());
    assert_send(async move { dispatch(&global, &handler, Ping).await });
}
