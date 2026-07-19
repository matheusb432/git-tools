# git-tools

`git-tools` is a Rust toolkit for repetitive Git workflows across one repository, nested repositories, or a managed repository set. It installs the `git-tools` command, its `gtl` alias, the resident `gtl-daemon`, and the `gtl-viewer` desktop application.

## Core workflows

- Render unpushed, range, merge, squash, recursive, or managed-repository diffs in the offline desktop viewer. Use `--raw` for a self-contained browser artifact.
- Inspect, commit, push, pull, prune, switch, tag, and squash local history without replacing Git's underlying repository model.
- Run status, commit, push, pull, and diff operations across repositories declared in `repos.toml`.

Run `gtl --help` and `gtl <command> --help` for the authoritative command reference. Architecture decisions are indexed in [docs/adr/adr.toml](docs/adr/adr.toml).

## Install

The supported development hosts are Ubuntu 24.04 and Windows 11 through Git Bash.

```sh
git clone git@github.com:matheusb432/git-tools.git
cd git-tools
sh xtask/bootstrap.sh
```

Bootstrap installs Rust when needed, prepares the repository toolchain, builds the CLI and viewer, and installs them on `PATH`. Run `just doctor` to diagnose missing host dependencies.

Refresh an existing installation with:

```sh
just update
```
