# git-tools

`git-tools` is a Rust toolkit for repetitive Git workflows across one repository, nested repositories, or a managed repository set. It installs the `git-tools` command, its `gtl` alias, the resident `gtl-server`, and the `gtl-viewer` desktop application.

## Core workflows

- Browse managed projects in the desktop app, ordered by their latest render, and open saved local-change or unpushed-commit comparisons.
- Render unpushed, range, merge, recursive, or managed-repository diffs in the offline desktop viewer. Use `--raw` for a self-contained browser artifact.
- Inspect status and worktrees; commit, push, pull, and manage tags without replacing Git's underlying repository model.
- Run status, commit, push, pull, and diff operations across active projects listed by sample_project.

Run `gtl --help` and `gtl <command> --help` for the authoritative command reference. ADR statuses are tracked in [docs/adr/adr.toml](docs/adr/adr.toml), and repository automation is indexed by `just --list` and the xtask CLI doc comments.

## Install

Development runs on Ubuntu 24.04. Windows 11 is a release target, not a development host.

```sh
git clone git@github.com:OWNER/git-tools.git
cd git-tools
mise trust
mise bootstrap --yes
```

Mise converges the declared Ubuntu packages, pinned tools, Rust toolchains, and zsh activation before it builds and installs the CLI and viewer. Run `just doctor` for a read-only report of missing declared state.

On Linux with systemd and on macOS, installation reconciles and starts `gtl-server` through the native user-service manager. The CLI discovers the resident server through its private local endpoint and capability token.

Refresh an existing installation with:

```sh
just update
```
