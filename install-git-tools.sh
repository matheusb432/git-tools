#!/usr/bin/env bash
# Ubuntu installer: build + place git-tools (CLI engine + desktop viewer) via the justfile,
# then ensure ~/.local/bin on PATH. Builds are owned by the just recipes; scripts/install.sh
# only copies the built artifacts to PATH, so this drives `just update` (build + install both).
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bindir="${HOME}/.local/bin"

# Builds live in the just recipes, so both tools must be on PATH. Fail loud with a
# fix hint rather than a cryptic "just: command not found" mid-build.
for tool in cargo just; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "error: '$tool' not found on PATH — required to build + install git-tools." >&2
    case "$tool" in
      cargo) echo "  install Rust: https://rustup.rs" >&2 ;;
      just)  echo "  install just: cargo install just  (or your package manager)" >&2 ;;
    esac
    exit 1
  }
done

(cd "$repo" && just update)

case ":$PATH:" in
  *":$bindir:"*) ;;
  *) echo "export PATH=\"$bindir:\$PATH\"" >> "${HOME}/.bashrc"
     echo "Added $bindir to PATH in ~/.bashrc (open a new shell)." ;;
esac
echo "git-tools installed: $(command -v git-tools || echo "$bindir/git-tools (reopen shell)")"
