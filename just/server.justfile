set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set positional-arguments
set working-directory := '..'

# Entries hl preloads before following, so a sparse filter still shows recent history.
_preload_entry_count := "2000"

_default:
    @just --list server

[doc("Read the installed gtl-server log stream through hl; trailing arguments go to hl and override the scope flags.
  own       --follow --tail " + _preload_entry_count + " --filter target~=gtl_server   gtl-server records only (default)
  all       --follow                                           every record, tower_http request lifecycle included
  failures  --follow --tail " + _preload_entry_count + " --level warn                  warnings and errors from every target
  history   --sort                                             current and rotated segments once, no follow")]
[arg("scope", help="Log scope", pattern="own|all|failures|history")]
[group('server')]
[linux]
logs scope="own" *args:
    #!/usr/bin/bash
    set -euo pipefail
    if ! command -v hl >/dev/null; then
        echo "just server logs: hl is not on PATH; install it from https://github.com/pamburus/hl" >&2
        exit 127
    fi
    log_directory="${XDG_STATE_HOME:-$HOME/.local/state}/git-tools/logs"
    log_file="$log_directory/gtl-server.jsonl"
    if [[ ! -f "$log_file" ]]; then
        echo "just server logs: no log at $log_file; start gtl-server first" >&2
        exit 1
    fi
    segments=("$log_file")
    case "{{ scope }}" in
        own) scope_args=(--follow --tail {{ _preload_entry_count }} --filter 'target~=gtl_server') ;;
        all) scope_args=(--follow) ;;
        failures) scope_args=(--follow --tail {{ _preload_entry_count }} --level warn) ;;
        history)
            scope_args=(--sort)
            while IFS= read -r segment; do segments+=("$segment"); done \
                < <(find "$log_directory" -maxdepth 1 -name 'gtl-server.jsonl.*' | sort)
            ;;
    esac
    status=0
    hl --config .config/hl.toml --local "${scope_args[@]}" "${@:2}" "${segments[@]}" || status=$?
    case "$status" in
        0 | 130 | 143) exit 0 ;;
        *) exit "$status" ;;
    esac
