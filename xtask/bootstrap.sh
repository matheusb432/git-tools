#!/bin/sh
# bootstrap.sh is the one shell responsibility: install the environment manager, then hand off.
# Mise owns Ubuntu packages, toolchains, and shell activation. Its bootstrap task invokes the
# repository-local Rust automation after Cargo is available.
set -eu

# Run from the repository root so mise discovers the tracked configuration regardless of where
# this script is invoked.
cd "$(dirname "$0")/.." || exit 1

mise_version="2026.7.18"
mise_binary_path="${MISE_INSTALL_PATH:-$HOME/.local/bin/mise}"
installed_version=""
if [ -x "$mise_binary_path" ]; then
    installed_version=$("$mise_binary_path" --version 2>/dev/null || true)
fi

case "$installed_version" in
    "$mise_version "*) ;;
    *)
        if ! command -v curl >/dev/null 2>&1; then
            echo "curl is required to install mise" >&2
            exit 1
        fi
        echo "installing mise $mise_version" >&2
        curl --proto '=https' --tlsv1.2 -sSf https://mise.run \
            | MISE_VERSION="v$mise_version" MISE_INSTALL_PATH="$mise_binary_path" sh
        ;;
esac

"$mise_binary_path" trust --yes "$PWD/mise.toml"
exec "$mise_binary_path" bootstrap --yes "$@"
