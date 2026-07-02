//! Missing `error = ...` in `#[request(...)]` must be a compile error.

use cqrs::Request;

#[derive(Request)]
#[request(response = ())]
struct Broken;

fn main() {}
