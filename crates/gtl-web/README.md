# gtl-web

`gtl-web` owns the production Dioxus Web application embedded in the Tauri viewer and the shared
presentation used by native raw-artifact rendering. It owns the workspace, diff history, read-only
user settings, application navigation, transient desktop state, and static artifact SSR contract.

The desktop backend remains authoritative for durable tabs, history, settings, Git decisions, and
raw diff lines. Shared Rust DTOs define focused Tauri commands and queries. The desktop hook loads
bounded line pages, retains parser state, and renders immutable row batches. Native artifact builds
drain the same typed paging contract, parse complete sources, and SSR the same row components before
writing HTML.

## Runtime and development

Release builds contain only local application assets and require no runtime network. The typed
xtask owns stylesheet generation, the Dioxus Web bundle, Tauri embedding, drift checks, and the
development server. Use the repository's `just --list` output for current entry points.

`src/app/assets/styles/tailwind.css` and `tokens.css` are the shared stylesheet sources. Rust owns
desktop browser behavior and all presentation. `src/artifact.js` is the one handwritten frontend
asset: a bounded progressive enhancer for already-rendered raw artifact markup.
