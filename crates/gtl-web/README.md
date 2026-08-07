# gtl-web

`gtl-web` is the development-only Dioxus Web shell for the desktop viewer proof. Dioxus owns two demonstration routes and transient shell state. The desktop backend still renders the diff island.

The proof runs behind the `dioxus-poc` feature with a separate Tauri identity. The production viewer continues to load its Maud and HTMX document from the `gtl://app` custom protocol.

## Run the proof

Start the Dioxus dev server, TypeScript bridge watcher, and Tauri host:

```sh
just desktop dioxus-poc
```

From another terminal in this repository, send the last committed diff to the sibling debug viewer:

```sh
mise exec -- cargo run --quiet -p gtl-cli -- diff --last 1
```

Use the debug CLI command for this proof. The installed `gtl` command resolves the installed production viewer instead.

RSX and Rust logic reload through the Dioxus dev server. `tailwind.css` drives the generated shell stylesheet, and `frontend/dioxus-poc/` owns the watched bridge bundle.

See [the Dioxus Web proof agent guide](../../docs/agents/dioxus-web-poc.md) for the runtime boundary, ownership map, and current limitations.
