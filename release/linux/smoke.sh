#!/usr/bin/env bash
set -euo pipefail

source_directory=$(cd -- "${1:?pass the extracted release directory}" && pwd)
smoke_root=$(mktemp -d -t 'git-tools release.XXXXXXXX')
server_pid=
cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -rf -- "$smoke_root"
}
trap cleanup EXIT
mkdir "$smoke_root/Local Tools" "$smoke_root/repository with spaces"
for binary in git-tools gtl gtl-server gtl-viewer; do
    install -m 755 "$source_directory/$binary" "$smoke_root/Local Tools/$binary"
done
binary_directory="$smoke_root/Local Tools"
export GIT_TOOLS_DATA_DIR="$smoke_root/data"
export GIT_TOOLS_CONFIG="$smoke_root/config.toml"
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL="$smoke_root/empty.gitconfig"
export GIT_TOOLS_NO_OPEN=1
touch "$GIT_CONFIG_GLOBAL"
"$binary_directory/git-tools" --version
"$binary_directory/gtl" --help >/dev/null
"$binary_directory/gtl-server" --version
"$binary_directory/gtl-server" >"$smoke_root/server.log" 2>&1 &
server_pid=$!
ready=false
for ((attempt = 0; attempt < 100; attempt++)); do
    if "$binary_directory/gtl" server status >/dev/null 2>&1; then
        ready=true
        break
    fi
    sleep 0.1
done
if [[ "$ready" != true ]]; then
    cat "$smoke_root/server.log" >&2
    exit 1
fi
cd "$smoke_root/repository with spaces"
git init -q -b main
git config user.name 'Git Tools smoke'
git config user.email git-tools-smoke@example.invalid
git config commit.gpgsign false
printf 'original\n' >sample.txt
git add sample.txt
git commit -qm original
printf 'changed\n' >sample.txt
git commit -qam changed
"$binary_directory/gtl" status --color never
artifact_url=$("$binary_directory/gtl" diff --raw --last 1)
[[ "$artifact_url" == file:///* ]]
artifact_path=${artifact_url#file://}
grep -q 'sample.txt' "$artifact_path"
printf '%s\n' 'Packaged Linux CLI, alias, daemon, and raw diff smoke passed.'
