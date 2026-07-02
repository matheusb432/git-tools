#!/bin/sh
# bootstrap.sh — the ONE shell responsibility: ensure the Rust toolchain exists, then hand off.
# This is the permanent toolchain-bootstrap seam (ADR-0010): you cannot `cargo run` the cargo
# you are about to install (chicken-and-egg), so this stays a thin, zero-dependency POSIX `sh`
# installer. Everything *after* the toolchain (deps, npm install, fixtures, links) lives in the
# Rust `xtask bootstrap` verb.
set -eu

# Run from the repo root so `cargo run -p xtask` resolves the workspace and the verb's CWD
# (skills link, build, install) is the repo root regardless of where this script is invoked.
cd "$(dirname "$0")/.." || exit 1

if ! command -v cargo >/dev/null 2>&1; then
    echo "installing Rust toolchain via rustup…" >&2
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi

exec cargo run --quiet -p xtask -- bootstrap "$@"
