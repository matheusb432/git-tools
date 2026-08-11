set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '../crates/gtl-web'

_default:
    @just --list web

# Build the tracked shell styles and stage the offline Dioxus release bundle.
[group('web')]
build:
    cargo run --quiet -p xtask -- web-build

# Regenerate the tracked shared Tailwind stylesheet.
[group('web')]
styles:
    cargo run --quiet -p xtask -- web-styles

# Serve the Dioxus Web app with Rust and stylesheet watchers.
[group('web')]
serve *args:
    cargo run --quiet -p xtask -- web-serve {{ args }}
