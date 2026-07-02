//! Compile-time contract tests for `#[derive(Mediator)]` — one valid use, two rejected shapes.
//! Run `TRYBUILD=overwrite cargo test -p cqrs-macros --test ui` to (re)generate `.stderr` files
//! after intentionally changing a diagnostic message.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_*.rs");
    t.compile_fail("tests/ui/fail_*.rs");
    t.compile_fail("tests/ui/request_*.rs");
}
