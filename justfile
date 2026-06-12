set shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]

_binname := if os() == "windows" { "git-tools.exe" } else { "git-tools" }
_bin := justfile_directory() / "target" / "release" / _binname
_install := justfile_directory() / "scripts" / "install.ps1"
_manifest := justfile_directory() / "git-tools.json"

_default:
    @just --list --unsorted

# Print the git-tools command reference.
help: _preflight
    @& "{{_bin}}" --help

# ============ build / install ============

# Build the release binary at target/release/git-tools[.exe].
[group('build')]
build:
    cargo build --release

# Build only if the binary is missing (preflight for run recipes).
_preflight:
    if (-not (Test-Path "{{_bin}}")) { cargo build --release }

# Install or refresh the binary/shim on PATH (Windows + Ubuntu).
[group('build')]
install:
    if ($IsWindows) { \
      if (-not (Get-Command scoop -ErrorAction SilentlyContinue)) { throw "scoop not found on PATH" } \
      $current = Join-Path $HOME "scoop/apps/git-tools/current/git-tools.exe"; \
      if (Test-Path -LiteralPath $current) { \
        cargo build --release; \
        Copy-Item -LiteralPath "{{_bin}}" -Destination $current -Force; \
        Write-Host "git-tools updated -> $current"; \
      } else { \
        scoop install "{{_manifest}}"; \
      } \
    } else { \
      & "{{_install}}" -Action install; \
    }

# Remove the installed binary/shim. Add -RemoveConfig to also delete git-tools.toml/secrets (confirms).
[group('build')]
uninstall *args:
    if ($IsWindows -and (Get-Command scoop -ErrorAction SilentlyContinue) -and (Test-Path -LiteralPath (Join-Path $HOME "scoop/apps/git-tools/current/git-tools.exe"))) { \
      scoop uninstall git-tools; \
      if ("{{args}}" -ne "") { & "{{_install}}" -Action uninstall {{args}}; } \
    } else { \
      & "{{_install}}" -Action uninstall {{args}}; \
    }

# Rebuild and refresh the installed binary in place (no re-link).
[group('build')]
update: build
    if ($IsWindows) { \
      if (-not (Get-Command scoop -ErrorAction SilentlyContinue)) { throw "scoop not found on PATH" } \
      $current = Join-Path $HOME "scoop/apps/git-tools/current/git-tools.exe"; \
      if (-not (Test-Path -LiteralPath $current)) { scoop install "{{_manifest}}" } else { \
        Copy-Item -LiteralPath "{{_bin}}" -Destination $current -Force; \
        Write-Host "git-tools updated -> $current"; \
      } \
    } else { \
      & "{{_install}}" -Action install; \
    }

# ============ quality ============

# cargo tests + the install Pester suite.
[group('quality')]
test: _preflight
    cargo test
    Invoke-Pester -Path "{{justfile_directory()}}/scripts/install.Tests.ps1" -CI

# Format all TOML with taplo (no-op if taplo is absent).
[group('quality')]
fmt:
    if (Get-Command taplo -ErrorAction SilentlyContinue) { taplo fmt } else { Write-Warning "taplo not installed; skipping" }

# Check TOML formatting without writing.
[group('quality')]
fmt-check:
    if (Get-Command taplo -ErrorAction SilentlyContinue) { taplo fmt --check } else { Write-Warning "taplo not installed; skipping" }
_linker := env('USERPROFILE', env('HOME', '')) / "shared" / "repo-bootstrap" / "link-claude-skills.ps1"

# One-time repo setup: link .claude/skills -> .agents/skills so Claude Code sees cross-agent skills.
bootstrap *flags:
    if (Test-Path "{{_linker}}") { & "{{_linker}}" -Repo "{{justfile_directory()}}" {{flags}} } else { [Console]::Error.WriteLine("deploy the helper first: sample_project 'just sync-shared'"); exit 1 }
