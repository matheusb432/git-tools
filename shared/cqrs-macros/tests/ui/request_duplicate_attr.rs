//! Two separate `#[request(...)]` attributes on the same struct must be a compile error.

use cqrs::Request;

#[derive(Request)]
#[request(response = (), error = std::io::Error)]
#[request(response = (), error = std::io::Error)]
struct Broken;

fn main() {}
