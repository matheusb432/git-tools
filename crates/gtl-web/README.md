# gtl-web

`gtl-web` is the production Dioxus Web application embedded in the Tauri viewer. It owns the
workspace, diff history, read-only user settings, application navigation, and transient shell
state.

The desktop backend remains authoritative for durable tabs, history, settings, Git decisions, and
rendering. Shared Rust DTOs define focused Tauri commands and queries. The active diff is installed
in one open shadow root: Rust renders its Maud file document and bounded row chunks, while the web
application owns the surrounding shell.

## Runtime and development

Release builds contain only local application assets and require no runtime network. The typed
xtask owns stylesheet generation, the Dioxus Web bundle, Tauri embedding, drift checks, and the
development server. Use the repository's `just --list` output for current entry points.

`src/app/assets/styles/tailwind.css` is the shell stylesheet source. The framework-free adapter in
`frontend/diff-island/` mounts the opaque server-rendered diff document and appends typed chunk
responses; it does not own application state or render rows.
