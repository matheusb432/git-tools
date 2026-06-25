# git-tools

A small Rust CLI for git workflow previews and local-history housekeeping. Installs as `git-tools` with a short `gtl` alias.

## Features

Per-repo:

- `diff` — render an HTML diff of the current repo: unpushed work (the default), a base commit, an exact `<start>..<end>` range, the last N commits (`-l N`), or a three-dot merge preview (`-m/--merge <base>`).
- `squash-preview` / `merge-diff` — render HTML previews of a subrepo's unpushed work or a three-dot merge diff against a base branch.
- `diff subrepos` — render one tabbed HTML diff for every git repo under the current directory.
- `squash-local` — squash all unpushed local commits into one (`--dry` to preview).
- `up` — stage, commit, and push the current repo (confirms first; `-y` to skip).
- `tag` — list tags with fetched origin status, create annotated tags with `tag add <tag> <message>`, show tag commits with `-c`/`--commits`, or create-and-push with `tag up <tag> <message>`.

Across a set of managed repos (declared in a `repos.toml` manifest of `[[repo]]` tables with `path` + `remote`):

- `diff --all` — render one tabbed HTML preview of unpushed commits across every managed repo that has them.
- `status` (alias `ls`) — branch, unpushed commits, and pending changes for every repo (`--json` for machine output).
- `push-all` / `pull-all` / `commit-all` — fan out push, pull, or commit across the set.

The managed manifest is resolved from `--repos-file`, then the `GIT_TOOLS_MANAGED_REPOS_FILE` environment variable, then an upward search for `config/provisioning/linux/repos.toml` from the current directory, then `$HOME/self/sample_project/config/provisioning/linux/repos.toml`.

Every HTML preview is a single self-contained **offline** file (opens from `file://`, no network): fast on large diffs (offscreen file blocks are deferred via CSS `content-visibility`), theme-switchable, and with per-file copy buttons for the relative path, the absolute path, and the code with diff `+`/`-` markers stripped.

## Where previews are saved

Previews are written to a central, app-owned store — **never into the repo being diffed**, so they can't be accidentally committed or pollute repos that don't gitignore them. The default location is your platform data dir (`~/.local/share/git-tools/diffs/` on Linux, `%LOCALAPPDATA%\git-tools\diffs\` on Windows), laid out as:

```
<data-dir>/diffs/<repo-id>/<content-hash>.html   # the self-contained preview
<data-dir>/diffs/<repo-id>/<content-hash>.json   # sidecar metadata (repo, range, timestamps)
```

`<repo-id>` is derived from the repo's root commit (stable across clone/move/rename). Set `GIT_TOOLS_DATA_DIR` to override the store root.

Rendering is **idempotent**: an identical diff reuses its existing artifact (addressed by content hash), and re-running on the same committed range skips re-rendering entirely.

After writing, the preview opens in the `gtl-viewer` desktop app (the default). `gtl-viewer` is a tray-resident Tauri window with a tab strip — each `gtl diff` opens or focuses a tab; the history panel lets you reopen past diffs grouped by repo. Control this behaviour with the `diff.viewer` config key:

| Value | Behaviour |
|---|---|
| `app` | Open in the `gtl-viewer` desktop app (default). Falls back to the browser automatically when no display is available (`$DISPLAY`/`$WAYLAND_DISPLAY` both absent), so `gtl diff` always succeeds in headless/CI environments. |
| `browser` | Open the artifact file directly in the OS default browser. |
| `none` | Write the artifact without opening anything (path is printed). |

Set `GIT_TOOLS_NO_OPEN=1` to suppress opening for a single run regardless of the config. The config file lives at `~/.config/git-tools/config.toml` (override with `XDG_CONFIG_HOME` or `GIT_TOOLS_CONFIG`).

```toml
# ~/.config/git-tools/config.toml
[diff]
viewer = "app"    # app | browser | none
```

Run `git-tools --help` (or `gtl --help`) for the full reference.

## Build

```sh
just build          # build both: CLI engine (+ diff bundle) and the desktop viewer
just cli build      # only the CLI engine -> target/release/git-tools[.exe]
just desktop build  # only the gtl-viewer Tauri binary (skipped without webkit2gtk-4.1 headers)
```

`just` recipes use bash, so on Windows run them from Git Bash (the `set windows-shell` directive points `just` at bash there).

To produce **Windows 11 release binaries from a Linux host** (no Windows machine needed for the build itself), cross-compile with [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin):

```sh
just ship          # cross-build both Win11 exes -> target/x86_64-pc-windows-msvc/release/{git-tools,gtl-viewer}.exe
just ship --smoke  # fast debug-profile linkage check (not a shippable)
```

This needs `cargo-xwin` and the `x86_64-pc-windows-msvc` rustup target; the verb's preflight prints the install command if either is missing. A cross-build proves the project *links* for Windows — WebView2 rendering, file dialogs, and the `%LOCALAPPDATA%` store path are certified separately on a real Win11 machine via `just win-release-checklist` (the host/release split).

## Install

The CLI engine (`git-tools` + `gtl` alias) and the desktop viewer (`gtl-viewer`) are independent artifacts with parallel verbs:

```sh
just update          # build + install both (CLI engine and desktop viewer)
just cli update      # build + install only the CLI engine (git-tools + gtl)
just desktop update  # build + install only the desktop viewer (gtl-viewer)
just install         # place both prebuilt artifacts on PATH (~/.local/bin)
just uninstall       # remove the binaries and the gtl alias
```

These wrap `scripts/install.sh`, which works on Linux and on Windows via Git Bash (it handles the `.exe` suffix under MSYS). The viewer is copied via an atomic `mv`, so `just desktop update` refreshes the running tray app in place without a "Text file busy" failure. On Ubuntu, `./install-git-tools.sh` additionally ensures `~/.local/bin` is on your `PATH`.

Override the install directory with `GIT_TOOLS_BINDIR` if you do not want `~/.local/bin`.
