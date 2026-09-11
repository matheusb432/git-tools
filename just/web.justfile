set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '../crates/gtl-web'

_default:
    @just --list web

# Build the tracked shell styles and stage the offline Dioxus release bundle.
[group('web')]
build:
    cargo run --quiet -p xtask -- web-build

# Regenerate the tracked desktop and artifact Tailwind stylesheets.
[group('web')]
styles:
    cargo run --quiet -p xtask -- web-styles

# Serve the Dioxus Web app with Rust and stylesheet watchers.
[group('web')]
serve *args:
    cargo run --quiet -p xtask -- web-serve {{ args }}

# Regenerate the tracked component-preview Tailwind stylesheet.
[group('web')]
preview-styles:
    cd ../.. && cargo run --quiet --manifest-path ../../shared-libs/dx-story/Cargo.toml -p dx-story-cli -- styles
