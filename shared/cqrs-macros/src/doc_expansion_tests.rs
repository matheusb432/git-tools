//! Pins the "maps to exactly this impl" fenced doc examples in `lib.rs` to the real macro output,
//! so the two can't silently drift apart again (they already have once).
//!
//! Each test extracts the fenced snippet that follows a given doc line, strips the `///`
//! doc-comment prefix so it parses as plain Rust, and compares it — normalized through `syn` —
//! against the token stream the real expander produces for the same input struct shown earlier
//! in the doc.

use quote::quote;
use syn::{DeriveInput, ItemImpl, parse_quote, parse_str, parse2};

use crate::{mediator::expand_mediator, request::expand_request};

const LIB_RS: &str = include_str!("lib.rs");

/// Finds the doc line containing `marker`, then extracts the ```ignore fenced block that follows
/// it, stripping the `///` doc-comment prefix from every line.
fn doc_ignore_block_after(marker: &str) -> String {
    let mut lines = LIB_RS.lines().skip_while(|line| !line.contains(marker));
    assert!(
        lines.next().is_some(),
        "no doc line in lib.rs contains {marker:?} — did the doc text move or get reworded?"
    );

    let strip_doc_prefix = |line: &str| {
        line.trim_start()
            .trim_start_matches("///")
            .strip_prefix(' ')
            .unwrap_or("")
            .to_string()
    };

    let mut in_block = false;
    let mut block = String::new();
    for line in lines {
        let stripped = strip_doc_prefix(line);
        if !in_block {
            if stripped == "```ignore" {
                in_block = true;
            }
            continue;
        }
        if stripped == "```" {
            return block;
        }
        block.push_str(&stripped);
        block.push('\n');
    }
    panic!("```ignore block after {marker:?} in lib.rs was never closed with a ``` fence");
}

/// Renders a parsed impl back through `quote!` so token-equal-but-differently-spaced inputs
/// compare equal.
fn normalize(item: &ItemImpl) -> String {
    quote!(#item).to_string()
}

#[test]
fn mediator_doc_expansion_matches_real_expansion() {
    let doc_block =
        doc_ignore_block_after("`#[handles(Ping)]` attribute above maps to exactly this impl");
    let doc_impl: ItemImpl = parse_str(&doc_block).unwrap_or_else(|err| {
        panic!("lib.rs's Mediator doc block failed to parse as an impl: {err}\n{doc_block}")
    });

    // The same `AppMediator` shape shown in the preceding doctest.
    let input: DeriveInput = parse_quote! {
        struct AppMediator {
            #[handles(Ping)]
            ping: PingHandler,
        }
    };
    let real_impl: ItemImpl = parse2(expand_mediator(&input).expect("expansion succeeds"))
        .expect("real Mediator expansion parses as a single impl");

    assert_eq!(
        normalize(&doc_impl),
        normalize(&real_impl),
        "lib.rs's Mediator `maps to exactly this impl` example has drifted from the real macro \
         expansion — update the ```ignore block in shared/cqrs-macros/src/lib.rs to match"
    );
}

#[test]
fn request_doc_expansion_matches_real_expansion() {
    let doc_block = doc_ignore_block_after(
        "`#[request(response = &'static str, error = Infallible)]` attribute above maps to exactly",
    );
    let doc_impl: ItemImpl = parse_str(&doc_block).unwrap_or_else(|err| {
        panic!("lib.rs's Request doc block failed to parse as an impl: {err}\n{doc_block}")
    });

    // The same `#[request(...)] struct Ping;` shown in the preceding doctest.
    let input: DeriveInput = parse_quote! {
        #[request(response = &'static str, error = Infallible)]
        struct Ping;
    };
    let real_impl: ItemImpl = parse2(expand_request(&input).expect("expansion succeeds"))
        .expect("real Request expansion parses as a single impl");

    assert_eq!(
        normalize(&doc_impl),
        normalize(&real_impl),
        "lib.rs's Request `maps to exactly this impl` example has drifted from the real macro \
         expansion — update the ```ignore block in shared/cqrs-macros/src/lib.rs to match"
    );
}
