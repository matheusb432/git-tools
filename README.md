# git-tools

`git-tools` is a Rust toolkit for repetitive Git workflows across one repository, nested repositories, or a managed repository set. It installs the `git-tools` command, its `gtl` alias, the resident `gtl-server`, and the `gtl-viewer` desktop application.

## Core workflows

- Find Git repositories recursively from a chosen folder and add selected projects in the desktop app. Browse managed projects, ordered by their latest render, and open saved local-change or unpushed-commit comparisons.
- Render unpushed, range, merge, recursive, or managed-repository diffs in the offline desktop viewer. Use `--raw` for a self-contained HTML document with unified diffs and expandable long lines, readable without JavaScript.
- Use the desktop viewer and its offline diffs in English (US) or Brazilian Portuguese.
- Review and confirm pushes of an exact commit from desktop diffs or the Projects dashboard.
- Inspect status, push changes, fast-forward branches, and manage tags.
- Run batch workflows across active [managed projects](docs/agents/projects.md), or select one project's repository by ID.

Run `gtl --help` and `gtl <command> --help` for the authoritative command reference. ADR statuses are tracked in [docs/adr/adr.toml](docs/adr/adr.toml), and repository automation is indexed by `just --list` and the xtask CLI doc comments.

## Install

For macOS, download the `.pkg` from a successful run of the
[macos workflow](https://github.com/matheusb432/git-tools/actions/workflows/macos.yml). Select
the artifact for your Mac's architecture. The installer adds the desktop app to `/Applications`,
the CLI commands to `/usr/local/bin`, and a background service that starts at login. Git must
already be installed. Keep `gtl-viewer.app` in `/Applications` so the commands and service can
find their binaries.

The macOS app uses ad-hoc signing and is not notarized. Allow installation and first launch in
System Settings -> Privacy & Security when macOS blocks them. The artifact includes a SHA-256
checksum and a screenshot from its native launch check.

Ubuntu 24.04 supports the full development and verification toolchain. macOS supports local
builds and installation through `just update`. Windows 11 is a release target.

On a Mac, install the Xcode Command Line Tools and have Git, Mise, and Deno available in your
shell. From this checkout, install the build tools and configure the local installation:

```sh
mise trust
mise install rust just protoc cargo-binstall cargo:dioxus-cli
just setup
```

`just setup` builds and installs the CLI, server, and standalone viewer in `~/.local/bin`,
registers the server in `~/Library/LaunchAgents`, and adds the binary directory to `.zshrc`
when needed. Open a new shell after setup. Subsequent `just update` runs rebuild all three
binaries and restart the user service. `GIT_TOOLS_BINDIR` can select another absolute install
directory, and Cargo's configured target directory is respected. Each install records the
invoking shell's PATH for launchd so the service can find developer tools at login.

For the full Ubuntu environment:

```sh
git clone git@github.com:OWNER/git-tools.git
cd git-tools
mise trust
mise bootstrap --yes
```

Mise converges the declared Ubuntu packages, pinned tools, Rust toolchains, and zsh activation before it builds and installs the CLI and viewer. Run `just doctor` for a read-only report of missing declared state.

On Linux with systemd and on macOS, installation reconciles and starts `gtl-server` through the native user-service manager. Native clients connect through a private Unix-domain socket on Linux and macOS or a per-user named pipe on Windows.

Refresh an existing installation with:

```sh
just update
```
