//! `#[handles(...)]` must name exactly one request type.

struct SomeHandler;
struct A;
struct B;

#[derive(cqrs::Mediator)]
struct Mediator {
    #[handles(A, B)]
    handler: SomeHandler,
}

fn main() {}
