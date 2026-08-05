# git-tools

`git-tools` is a Rust toolkit for repetitive Git workflows across one repository, nested repositories, or a managed repository set. It installs the `git-tools` command, its `gtl` alias, the resident `gtl-daemon`, and the `gtl-viewer` desktop application.

## Core workflows

- Render unpushed, range, merge, recursive, or managed-repository diffs in the offline desktop viewer. Use `--raw` for a self-contained browser artifact.
- Inspect, commit, push, pull, prune, switch, and tag without replacing Git's underlying repository model.
- Run status, commit, push, pull, and diff operations across repositories declared in `projects.toml`.

Run `gtl --help` and `gtl <command> --help` for the authoritative command reference. Architecture decisions are indexed in [docs/adr/adr.toml](docs/adr/adr.toml).

## Install

Development runs on Ubuntu 24.04. Windows 11 is a release target, not a development host.

```sh
git clone git@github.com:matheusb432/git-tools.git
cd git-tools
sh xtask/bootstrap.sh
```

Bootstrap installs mise when needed. Mise then converges the declared Ubuntu packages, pinned tools, Rust toolchains, and zsh activation before it builds and installs the CLI and viewer. Run `just doctor` for a read-only report of missing declared state.

Refresh an existing installation with:

```sh
just update
```
