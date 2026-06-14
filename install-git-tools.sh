#!/usr/bin/env bash
# Ubuntu installer: build + place git-tools via scripts/install.sh, then ensure ~/.local/bin on PATH.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bindir="${HOME}/.local/bin"

bash "$repo/scripts/install.sh" install

case ":$PATH:" in
  *":$bindir:"*) ;;
  *) echo "export PATH=\"$bindir:\$PATH\"" >> "${HOME}/.bashrc"
     echo "Added $bindir to PATH in ~/.bashrc (open a new shell)." ;;
esac
echo "git-tools installed: $(command -v git-tools || echo "$bindir/git-tools (reopen shell)")"
