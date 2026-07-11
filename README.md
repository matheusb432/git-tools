# git-tools

A small Rust CLI for git workflow previews and local-history housekeeping. Installs as `git-tools` with a short `gtl` alias.

## Features

Per-repo:

- `diff` — render an HTML diff of the current repo: unpushed work (the default), a base commit, an exact `<start>..<end>` range, the last N commits (`-l N`), or a three-dot merge preview (`-m/--merge <base>`). `diff -r` renders one tabbed diff for every git repo under the current directory.
- `diff merge --repo <r> [--base <b>]` / `diff squash --repo <r>` — merge and squash previews of a subrepo (the legacy `merge-diff`/`squash-preview` spellings still work).
- `squash-local` — squash all unpushed local commits into one (`--dry` to preview).
- `push` — push existing commits; with a message, stage all changes, commit, and push. Confirmation is required by default (`-y` skips it); `[push].confirm = false` disables it only for plain pushes.
- `commit` — stage all changes and commit without pushing.
- `tag` — list tags with fetched origin status, create annotated tags (`tag add <tag> <message>`), show tag commits (`-c`), or create-and-push (`tag up <tag> <message>`).
- `sw` — switch to the main branch (`--rebase` to fast-forward it onto your commits); `prune` — delete local branches already merged into main; `wk` — inspect worktrees.

Across a set of managed repos (declared in a `repos.toml` manifest of `[[repo]]` tables with `path` + `remote`):

- `diff --all` — one tabbed HTML preview of unpushed commits across every managed repo that has them.
- `status --all` — branch, unpushed commits, and pending changes for every repo (`--json` for machine output).
- `push --all` / `pull --all` / `commit --all` — fan out across the set.

The manifest is resolved from `--repos-file`, then `GIT_TOOLS_MANAGED_REPOS_FILE`, then an upward search for `repos.toml`, then `$HOME/tools/sample_project/repos.toml`.

Every HTML preview is a single self-contained **offline** file (opens from `file://`, no network): fast on large diffs (offscreen file blocks deferred via CSS `content-visibility`), theme-switchable, with per-file copy buttons for the relative path, the absolute path, and the code with diff `+`/`-` markers stripped.

## Where previews are saved

Previews go to a central, app-owned store — **never into the repo being diffed**. Default: your platform data dir (`~/.local/share/git-tools/diffs/` on Linux, `%LOCALAPPDATA%\git-tools\diffs\` on Windows), overridable with `GIT_TOOLS_DATA_DIR`:

```
<data-dir>/diffs/<repo-id>/<content-hash>.html   # the self-contained preview
<data-dir>/diffs/<repo-id>/<content-hash>.json   # sidecar metadata (repo, range, timestamps)
```

`<repo-id>` derives from the repo's root commit (stable across clone/move/rename). Rendering is **idempotent**: an identical diff reuses its artifact (addressed by content hash), and re-running the same committed range skips re-rendering.

The preview opens in the `gtl-viewer` desktop app by default — a tray-resident Tauri window with a tab strip (each `gtl diff` opens or focuses a tab) and a history panel of every past diff across all repos, newest first. Pass `--raw` to open the artifact in your browser instead; with no display or viewer binary, the CLI falls back to the browser automatically. `GIT_TOOLS_NO_OPEN=1` suppresses opening for a single run.

Config lives at `~/.config/git-tools/config.toml` (override with `XDG_CONFIG_HOME` or `GIT_TOOLS_CONFIG`):

```toml
theme = "dark" # dark | light | hearth

[push]
confirm = false # only plain `gtl push`; defaults to true
```

Run `gtl --help` for the full reference.

## Build

```sh
just build          # both: CLI engine (+ diff bundle + gtl-daemon) and the desktop viewer
just cli build      # only the CLI engine -> target/release/{git-tools,gtl-daemon}[.exe]
just desktop build  # only the gtl-viewer Tauri binary (skipped without webkit2gtk-4.1 headers)
```

Diff and managed-repo commands execute via a resident `gtl-daemon` on `127.0.0.1`; the CLI autostarts it on first use and restarts it after a version mismatch (e.g. after `just update`). `gtl daemon status` / `gtl daemon stop` inspect and terminate it — most users never need to.

`just` recipes use bash; on Windows run them from Git Bash (`set windows-shell` points `just` at bash there).

To build **Windows 11 release binaries from a Linux host**, cross-compile with [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin):

```sh
just ship          # all three Win11 exes -> target/x86_64-pc-windows-msvc/release/{git-tools,gtl-daemon,gtl-viewer}.exe
just ship --smoke  # fast debug-profile linkage check (not a shippable)
```

Needs `cargo-xwin` and the `x86_64-pc-windows-msvc` rustup target; the preflight prints the install commands if missing. A cross-build proves the project *links* — WebView2 rendering, file dialogs, and the `%LOCALAPPDATA%` store path are certified on a real Win11 machine via `just win-release-checklist`.

## Install

The CLI engine (`git-tools` + `gtl`) and the desktop viewer (`gtl-viewer`) are independent artifacts with parallel verbs:

```sh
just update          # build + install both
just cli update      # build + install only the CLI engine (git-tools + gtl + gtl-daemon)
just desktop update  # build + install only the desktop viewer
just install         # place all prebuilt artifacts on PATH (~/.local/bin)
just uninstall       # remove the binaries and the gtl alias
```

These work on Linux and on Windows via Git Bash (the `.exe` suffix is handled via `std::env::consts::EXE_SUFFIX`). The viewer and `gtl-daemon` are copied via atomic replace, so updates are safe while the tray app or daemon is running (no "Text file busy"). Override the install directory with `GIT_TOOLS_BINDIR`.

For a fresh machine, `sh xtask/bootstrap.sh` installs the Rust toolchain then runs the full bring-up (`just bootstrap`): build, install, and ensure `~/.local/bin` is on your `PATH`.
