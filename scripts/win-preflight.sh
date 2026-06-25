#!/usr/bin/env bash
# Sourced by the justfile `win-*` recipes (and reusable by a future xtask `ship` verb). Fails
# loud with a fix hint when the Linux->Windows cross toolchain is incomplete. Keep side-effect-free
# on load (only defines a function); Linux-first, no PowerShell.

GTL_WIN_TARGET="x86_64-pc-windows-msvc"

# Verify cargo-xwin and the rustup windows target are present. Returns non-zero with a printed fix
# hint on the first miss. llvm-lib/llvm-rc are surfaced by cargo-xwin itself if absent, so the two
# most common misses are checked here.
gtl_win_preflight() {
  if ! command -v cargo-xwin >/dev/null 2>&1; then
    echo "cargo-xwin not found — run: cargo install cargo-xwin --locked" >&2
    return 1
  fi
  if ! rustup target list --installed 2>/dev/null | grep -qx "$GTL_WIN_TARGET"; then
    echo "rustup target '$GTL_WIN_TARGET' missing — run: rustup target add $GTL_WIN_TARGET" >&2
    return 1
  fi
}
