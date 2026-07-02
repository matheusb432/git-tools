//! An empty `with()` list in `#[request(...)]` must be a compile error.

use cqrs::Request;

#[derive(Request)]
#[request(response = (), error = std::io::Error, with())]
struct Broken;

fn main() {}
