set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '../crates/gtl-web'

_default:
    @just --list web

# Build the tracked shell styles and stage the offline Dioxus release bundle.
[group('web')]
build:
    cargo run --quiet -p xtask -- web-build

# Regenerate the tracked shell and diff-island stylesheets.
[group('web')]
styles:
    cargo run --quiet -p xtask -- web-styles

# Serve the Dioxus Web app with typed Rust, TypeScript, and stylesheet watchers.
[group('web')]
serve *args:
    cargo run --quiet -p xtask -- web-serve {{ args }}
