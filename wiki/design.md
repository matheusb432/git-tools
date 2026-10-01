# Design

git-tools was built to keep the overhead of reviewing and updating repositories low.

I wanted to review local changes and unpushed commits in one place, then use the same commands
when working with nested repositories or a set of projects.

The app runs locally. A background server keeps the project catalogue, settings and diff
history so the CLI and viewer use the same data.

## Runtime overview

`gtl-cli` and `gtl-web` are frontend process roots: they translate terminal or viewer input into client requests and present results.
`gtl-server` is the backend process root: it wires concrete adapters, serves requests, and executes application operations.
Viewer requests cross the Tauri host before native gRPC reaches the server; the host is omitted from this overview.

```mermaid
flowchart TB
    subgraph frontends["Frontend process roots"]
        cli["gtl-cli<br/>Terminal commands"]
        web["gtl-web<br/>Dioxus viewer"]
    end
    client["gtl-client<br/>Client adapters"]
    server["gtl-server<br/>Backend process root"]
    application["gtl-application<br/>Use cases and ports"]
    infra["gtl-infra<br/>Concrete adapters"]
    artifacts["gtl-artifacts<br/>Offline HTML"]

    cli --> client
    web --> client
    client -->|"Native gRPC<br/>Viewer crosses Tauri IPC"| server
    server -->|"Executes"| application
    application -->|"Calls injected adapters"| infra
    server -->|"Renders"| artifacts
```

## Crate dependencies

`A → B` means crate A directly depends on crate B in Cargo.
This graph shows normal dependencies between the nine overview crates; dependencies on other workspace crates and external libraries are omitted.
Runtime calls above do not imply Cargo dependencies.

```mermaid
flowchart TB
    subgraph roots["Process roots"]
        cli["gtl-cli<br/>Frontend"]
        web["gtl-web<br/>Frontend"]
        server["gtl-server<br/>Backend"]
    end
    client["gtl-client"]
    infra["gtl-infra"]
    artifacts["gtl-artifacts"]
    application["gtl-application<br/>Use cases and ports"]
    wire["gtl-wire<br/>Shared contracts"]
    models["gtl-models<br/>Domain values"]

    cli --> client
    cli --> wire
    cli --> models
    web --> client
    web --> wire
    web --> models
    server --> application
    server --> infra
    server --> artifacts
    server --> wire
    server --> models
    client --> wire
    client --> models
    infra --> application
    infra --> models
    artifacts --> application
    artifacts --> models
    application --> wire
    application --> models
    wire --> models
```

## Crate roles

| Crate | Responsibility |
| --- | --- |
| `gtl-cli` | Frontend process root: parse CLI intent, prompt, format output, and choose exit status |
| `gtl-web` | Frontend process root: Dioxus routes, temporary UI state, and diff presentation |
| `gtl-server` | Backend process root: adapter composition, application execution, services, and viewer launch |
| `gtl-infra` | Concrete Git, persistence, filesystem, clock, and OS adapters |
| `gtl-models` | Domain values and typed failures |
| `gtl-wire` | Shared operation contracts and optional protobuf and gRPC codecs |
| `gtl-client` | Native gRPC and WebView IPC clients; typed failure decoding |
| `gtl-application` | Git use cases, viewer sessions, cache, diff projection, and capability ports |
| `gtl-artifacts` | Static HTML rendering, styles, and asset safety |
