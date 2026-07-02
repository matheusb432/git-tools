//! `#[derive(Mediator)]` only supports named-field structs.

#[derive(cqrs::Mediator)]
struct Mediator(u8);

fn main() {}
