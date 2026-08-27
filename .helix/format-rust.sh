#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
repository_relative_directory="$(git rev-parse --show-prefix)"

cd -- "${repository_root}"
if [[ "${repository_relative_directory}" == crates/gtl-web/src/* ]]; then
    rustfmt --edition 2024 | dx fmt --file -
else
    rustfmt --edition 2024
fi
