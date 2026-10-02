#!/bin/sh
set -eu
systemctl --user daemon-reload
/usr/bin/gtl server start
exec /usr/bin/gtl-viewer "$@"
