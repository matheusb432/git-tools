# gtl-web

`gtl-web` owns the production Dioxus Web application embedded in the Tauri viewer and the shared
presentation used by native raw-artifact rendering. It owns the workspace, diff history, read-only
user settings, application navigation, transient desktop state, and static artifact SSR contract.

`gtl-server` remains authoritative for durable tabs, history, settings, Git decisions, cached views,
and diff parsing. The WebView calls typed `gtl-client` operations through Tauri IPC; the desktop
forwards them through the authenticated native gRPC client. The WebView stores received rows only as
temporary display state. Native artifact builds ask `gtl-application` to parse complete files and SSR
the same row components before writing HTML.

## Runtime and development

Release builds contain only local application assets and connect only to the local `gtl-server`. The typed
xtask owns stylesheet generation, the Dioxus Web bundle, Tauri embedding, drift checks, and the
development server. Use the repository's `just --list` output for current entry points.

`src/app/assets/styles/` owns shared theme and component rules. The `tailwind.css` entry point
includes the whole application; `artifact.css` includes the shared diff and control rules needed
by raw HTML artifacts. Both generate tracked stylesheets in `assets/`. Rust owns
desktop browser behavior and all presentation. `src/artifact.js` is the one handwritten frontend
asset: a bounded progressive enhancer for already-rendered raw artifact markup. Artifact builds
minify it into Cargo's output directory before embedding it in the saved HTML.
