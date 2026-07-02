//! Proves the async `RequestHandler` surface already subsumes the synchronous case — so no
//! separate sync handler trait is needed.

use std::convert::Infallible;

use cqrs_core::{Request, RequestHandler};

struct Ping;
impl Request for Ping {
    type Response = i32;
    type Error = Infallible;
    type Pipeline = ();
}

struct PingHandler;
impl RequestHandler<Ping> for PingHandler {
    // A synchronous body (no `.await`) satisfies the async trait at zero cost.
    async fn handle(&self, _req: Ping) -> Result<i32, Infallible> {
        Ok(42)
    }
}

#[tokio::test]
async fn sync_bodied_handler_satisfies_request_handler() {
    assert_eq!(PingHandler.handle(Ping).await.unwrap(), 42);
}

#[test]
fn async_handler_is_driven_from_sync_code_via_block_on() {
    let rt = tokio::runtime::Runtime::new().expect("build runtime");
    assert_eq!(rt.block_on(PingHandler.handle(Ping)).unwrap(), 42);
}
