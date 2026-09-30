#!/usr/bin/env bash
set -euo pipefail

source_directory=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
binary_directory=${GIT_TOOLS_BINDIR:-$HOME/.local/bin}
case "$binary_directory" in
    /*) ;;
    *) printf '%s\n' 'GIT_TOOLS_BINDIR must be absolute' >&2; exit 1 ;;
esac
for binary in git-tools gtl gtl-server gtl-viewer; do
    test -s "$source_directory/$binary"
done
systemctl --user stop gtl-server.service 2>/dev/null || true
install -d "$binary_directory" "$HOME/.config/systemd/user"
for binary in git-tools gtl gtl-server gtl-viewer; do
    install -m 755 "$source_directory/$binary" "$binary_directory/$binary"
done
server=${binary_directory//%/%%}/gtl-server
server=${server//\\/\\\\}
server=${server//\"/\\\"}
cat > "$HOME/.config/systemd/user/gtl-server.service" <<EOF
[Unit]
Description=Git Tools local gRPC server
After=graphical-session.target

[Service]
ExecStart="$server"
Restart=on-failure
RestartSec=2s
UMask=0077

[Install]
WantedBy=graphical-session.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now gtl-server.service
printf 'Installed Git Tools in %s. Add this directory to PATH and run gtl-viewer.\n' "$binary_directory"
