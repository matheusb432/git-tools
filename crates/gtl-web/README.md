# gtl-web

`gtl-web` owns the Dioxus Web application embedded in the Tauri viewer: diff presentation,
history, settings, navigation, and temporary UI state. Component previews reuse its viewer controls.

`gtl-server` remains authoritative for durable tabs, history, settings, Git decisions, cached views,
and diff parsing. The WebView calls typed `gtl-client` operations through Tauri IPC; the desktop
forwards them through the private native gRPC client. The WebView stores received rows as temporary display state.

Offline HTML documents belong to `gtl-artifacts`, which has its own renderer and CSS and consumes shared parser and domain data.
`gtl-web` has no artifact build mode or artifact presentation branches.

## Runtime and development

Release builds contain local application assets and connect to the local `gtl-server`.
The typed xtask owns stylesheet generation, the Dioxus Web bundle, Tauri embedding, drift checks,
and the development server. Use `just --list` for current entry points.

`src/app/assets/styles/` owns viewer theme and component rules. The `tailwind.css` entry point
includes the application and generates its tracked stylesheet in `assets/`.
The `web` feature selects the browser runtime; `component-preview` adds the component catalog.
