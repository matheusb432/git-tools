# gtl-web

`gtl-web` is the production Dioxus Web application embedded in the Tauri viewer. It owns the
workspace, diff history, read-only user settings, application navigation, and transient shell
state.

The desktop backend remains authoritative for durable tabs, history, settings, Git decisions, and
raw diff lines. Shared Rust DTOs define focused Tauri commands and queries. A source-neutral Dioxus
hook loads bounded line pages, retains parser state, and renders immutable row batches. The raw
artifact uses the same hook and components against embedded typed pages.

## Runtime and development

Release builds contain only local application assets and require no runtime network. The typed
xtask owns stylesheet generation, the Dioxus Web bundle, Tauri embedding, drift checks, and the
development server. Use the repository's `just --list` output for current entry points.

`src/app/assets/styles/tailwind.css` and `tokens.css` are the shared stylesheet sources. Rust owns
all browser behavior and presentation; the repository has no handwritten JavaScript or TypeScript
frontend.
