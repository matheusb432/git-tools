//! A minimal request/handler pair, derived into a mediator — must compile.

use std::convert::Infallible;

#[derive(cqrs::Request)]
#[request(response = i32, error = Infallible)]
struct Ping;

#[derive(Clone)]
struct PingHandler;
impl cqrs::RequestHandler<Ping> for PingHandler {
    async fn handle(&self, _req: Ping) -> Result<i32, Infallible> {
        Ok(42)
    }
}

#[derive(Clone, cqrs::Mediator)]
struct Mediator {
    #[handles(Ping)]
    ping: PingHandler,
}

fn main() {}
