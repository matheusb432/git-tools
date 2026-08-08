set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set working-directory := '../crates/gtl-web'

_default:
    @just --list web

# Serve the Dioxus Web app in isolation with file watching and full hot reload.
[group('web')]
serve *args:
    dx serve --web --package gtl-web --locked --hot-reload true --watch true {{ args }}
