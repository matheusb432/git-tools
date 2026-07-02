//! Missing `response = ...` in `#[request(...)]` must be a compile error.

use cqrs::Request;

#[derive(Request)]
#[request(error = std::io::Error)]
struct Broken;

fn main() {}
