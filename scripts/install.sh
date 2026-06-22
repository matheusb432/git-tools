#!/usr/bin/env bash
# Install / uninstall the git-tools binary and the `gtl` alias on PATH. Idempotent;
# destructive config removal refuses non-interactively unless --force. Linux-first; runs
# on Windows under a POSIX shell (Git Bash / MSYS2). No PowerShell.
#
# Library functions are sourced by spec/install_spec.sh — keep them side-effect-free on
# load (no top-level `set`/work); main() runs only on direct execution.

# --- library (unit-tested by spec/install_spec.sh) -------------------------

# Executable suffix for the host: `.exe` under Git Bash/MSYS2/Cygwin, empty elsewhere.
cli_exe_suffix() {
  case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) printf '.exe' ;;
    *) printf '' ;;
  esac
}

cli_bin_name() { printf 'git-tools%s' "$(cli_exe_suffix)"; }
cli_alias_name() { printf 'gtl%s' "$(cli_exe_suffix)"; }
cli_bindir() { printf '%s' "${GIT_TOOLS_BINDIR:-$HOME/.local/bin}"; }

# SHA-256 of a file, portable across coreutils (sha256sum) and BSD/macOS (shasum).
_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# Copy $1 -> $2 only when the bytes differ. Echoes: installed | updated | unchanged.
copy_if_changed() {
  local src=$1 dst=$2
  if [ -e "$dst" ]; then
    if [ "$(_sha256 "$dst")" = "$(_sha256 "$src")" ]; then
      printf 'unchanged'
      return 0
    fi
    cp -f "$src" "$dst"
    printf 'updated'
    return 0
  fi
  cp -f "$src" "$dst"
  printf 'installed'
}

# Install source exe $1 plus the gtl alias into bindir $2. Echoes the combined action
# (updated wins over installed wins over unchanged), mirroring the old Install-CliBinary.
install_cli_binary() {
  local src=$1 bindir=$2 primary alias
  mkdir -p "$bindir"
  primary=$(copy_if_changed "$src" "$bindir/$(basename "$src")")
  alias=$(copy_if_changed "$src" "$bindir/$(cli_alias_name)")
  if [ "$primary" = updated ] || [ "$alias" = updated ]; then
    printf 'updated'
  elif [ "$primary" = installed ] || [ "$alias" = installed ]; then
    printf 'installed'
  else
    printf 'unchanged'
  fi
}

# Remove the binary + gtl alias from bindir $1. Echoes: removed | nothing.
uninstall_cli_binary() {
  local bindir=$1 removed=0 path
  for path in "$bindir/$(cli_bin_name)" "$bindir/$(cli_alias_name)"; do
    if [ -e "$path" ]; then
      rm -f "$path"
      removed=1
    fi
  done
  if [ "$removed" = 1 ]; then printf 'removed'; else printf 'nothing'; fi
}

# Destructive config delete, guarded. Refuses non-interactively unless --force ($2).
# Echoes: removed | kept | absent.
remove_cli_config() {
  local path=$1 force=0 ans
  [ "${2:-}" = "--force" ] && force=1
  [ -e "$path" ] || {
    printf 'absent'
    return 0
  }
  if [ "$force" != 1 ]; then
    if [ ! -t 0 ]; then
      printf "Delete config file '%s'? [refused: non-interactive, pass --force to proceed]\n" "$path" >&2
      printf 'kept'
      return 0
    fi
    printf "Delete config file '%s'? [y/N] " "$path" >&2
    read -r ans || true
    case "$ans" in
      y | Y | yes | Yes) ;;
      *)
        printf 'kept'
        return 0
        ;;
    esac
  fi
  rm -f "$path"
  printf 'removed'
}

# --- orchestration (runs only on direct execution) -------------------------

main() {
  set -euo pipefail
  local action=${1:-} remove_config=0 force=0 arg repo src bindir
  shift || true
  for arg in "$@"; do
    case "$arg" in
      --remove-config) remove_config=1 ;;
      --force) force=1 ;;
    esac
  done

  repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
  bindir=$(cli_bindir)
  src="$repo/target/release/$(cli_bin_name)"

  case "$action" in
    install)
      printf 'Building release binary...\n'
      cargo build --release --manifest-path "$repo/Cargo.toml"
      local act
      act=$(install_cli_binary "$src" "$bindir")
      printf 'git-tools %s -> %s\n' "$act" "$bindir/$(basename "$src")"
      printf 'gtl %s -> %s\n' "$act" "$bindir/$(cli_alias_name)"
      local viewer_src="$repo/target/release/gtl-viewer$(cli_exe_suffix)"
      if [ -f "$viewer_src" ]; then
        local viewer_act
        viewer_act=$(copy_if_changed "$viewer_src" "$bindir/gtl-viewer$(cli_exe_suffix)")
        printf 'gtl-viewer %s -> %s\n' "$viewer_act" "$bindir/gtl-viewer$(cli_exe_suffix)"
      else
        printf 'gtl-viewer not built (viewer is optional; browser fallback active)\n' >&2
      fi
      ;;
    uninstall)
      local result
      result=$(uninstall_cli_binary "$bindir")
      if [ "$result" = removed ]; then
        printf 'removed %s\n' "$bindir/$(cli_bin_name)"
      else
        printf 'nothing to remove at %s\n' "$bindir/$(cli_bin_name)"
      fi
      local viewer_dst="$bindir/gtl-viewer$(cli_exe_suffix)"
      if [ -e "$viewer_dst" ]; then
        rm -f "$viewer_dst"
        printf 'removed %s\n' "$viewer_dst"
      fi
      if [ "$remove_config" = 1 ]; then
        local cfg flag=''
        [ "$force" = 1 ] && flag='--force'
        for cfg in git-tools.toml git-tools.secrets.toml; do
          remove_cli_config "$PWD/$cfg" $flag >/dev/null
        done
      fi
      ;;
    *)
      printf 'usage: install.sh install | uninstall [--remove-config] [--force]\n' >&2
      exit 2
      ;;
  esac
}

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  main "$@"
fi
