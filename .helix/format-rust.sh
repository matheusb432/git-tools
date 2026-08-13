#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
repository_relative_directory="$(git rev-parse --show-prefix)"
rustfmt_toolchain="$(tr -d '[:space:]' < "${repository_root}/.rustfmt-nightly")"

cd -- "${repository_root}"
if [[ "${repository_relative_directory}" == crates/gtl-web/src/* ]]; then
    rustup run "${rustfmt_toolchain}" rustfmt --edition 2024 | dx fmt --file -
else
    rustup run "${rustfmt_toolchain}" rustfmt --edition 2024
fi
