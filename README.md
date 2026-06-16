# git-tools

A small Rust CLI for git workflow previews and local-history housekeeping. Installs as `git-tools` with a short `gtl` alias.

## Features

Per-repo:

- `diff` — render an HTML diff of the current repo: unpushed work (the default), a base commit, an exact `<start>..<end>` range, or the last N commits (`-l N`).
- `squash-preview` / `diff-subrepos` / `merge-diff` — render HTML previews of a subrepo's unpushed work, per-subrepo diffs, or a three-dot merge diff against a base branch.
- `squash-local` — squash all unpushed local commits into one (`--dry` to preview).
- `sync` — stage, commit, and push the current repo (confirms first; `-y` to skip).

Across a set of managed repos (declared in a tab-separated `local-path<TAB>git-remote` manifest):

- `diff --all` — render one tabbed HTML preview of unpushed commits across every managed repo that has them.
- `status` (alias `ls`) — branch, unpushed commits, and pending changes for every repo (`--json` for machine output).
- `push-all` / `pull-all` / `commit-all` — fan out push, pull, or commit across the set.

The managed manifest is resolved from `--repos-file`, then the `GIT_TOOLS_MANAGED_REPOS_FILE` environment variable, then an upward search for `config/provisioning/linux/repos.txt` from the current directory.

Run `git-tools --help` (or `gtl --help`) for the full reference.

## Build

```sh
just build      # cargo build --release -> target/release/git-tools[.exe]
```

`just` recipes use bash, so on Windows run them from Git Bash (the `set windows-shell` directive points `just` at bash there).

## Install

```sh
just install    # build + place the binary and the gtl alias on PATH (~/.local/bin)
```

`just install` / `just uninstall` / `just update` wrap `scripts/install.sh`, which works on Linux and on Windows via Git Bash (it handles the `.exe` suffix under MSYS). On Ubuntu, `./install-git-tools.sh` additionally ensures `~/.local/bin` is on your `PATH`.

Override the install directory with `GIT_TOOLS_BINDIR` if you do not want `~/.local/bin`.
