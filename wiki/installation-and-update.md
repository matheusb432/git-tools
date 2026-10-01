# Installation and updates

Download the archive or installer for your platform from
[GitHub Releases](https://github.com/matheusb432/git-tools/releases), verify its accompanying
SHA-256 checksum, then follow the included [installation instructions](../release/INSTALL.md).

The packages include `git-tools`, its `gtl` alias, `gtl-server` and `gtl-viewer`. Git must already
be on PATH.

| Platform | Package |
| -- | -- |
| Ubuntu 24.04 x64 | `.tar.gz` with `install.sh` |
| Windows 11 x64 | ZIP with `install.cmd`; requires WebView2 |
| macOS | `.pkg` for Apple Silicon or Intel |

The macOS app is ad-hoc signed and is not notarized. Allow installation and first launch in
System Settings → Privacy & Security when macOS blocks them.

After installation:

```bash
gtl --help
gtl server status
```

> [!TIP]
> Use `gtl {noun/verb} -h/--help` to get usage help on any command.

## Building from source

Ubuntu 24.04 supports the full development and verification toolchain. macOS supports local
builds and installation through `just update`. Windows 11 is a release target.

Cargo also needs the `error-meta` checkout at `../../shared-libs/error-meta`, relative to this
repository. Keep that directory layout when building from source.

With Mise installed, run these from this checkout on Ubuntu:

```bash
mise trust
mise bootstrap --yes # installs the declared tools and packages, then builds and installs git-tools
```

On macOS, install the Xcode Command Line Tools and have Git and Mise available in your shell:

```bash
mise trust
mise install rust just protoc cargo-binstall cargo:dioxus-cli http:zig
just setup
```

Source installs put the executables in `~/.local/bin`. `just setup` adds that directory to
`.zshrc` when needed; open a new shell afterwards. Set `GIT_TOOLS_BINDIR` to an absolute path
to use another install directory.

## Updating

Run the installer from the new package to update a packaged installation. From a source checkout:

```bash
just update # rebuilds and installs the CLI, server and viewer
```

Source installation registers and starts the server with the systemd user service on Linux or a
launch agent on macOS. Updating also restarts it. See [troubleshooting](troubleshooting.md) for
the server commands. Use `just --list` for the current build and verification commands.

## Data and config

Projects and diff history stay in a local SQLite database, `gtl.db`, under the platform's
git-tools data directory. On Linux this is usually `~/.local/share/git-tools`. The CLI and
viewer call the server through a private Unix socket or a Windows named pipe.

User settings live in `~/.config/git-tools/config.toml`, or under `XDG_CONFIG_HOME` when set.
On Windows the default is `%USERPROFILE%\.config\git-tools\config.toml`. You can edit settings
through the viewer or by hand; see [config.example.toml](../config/local/config.example.toml)
for examples.

| Variable | Use |
| -- | -- |
| `GIT_TOOLS_CONFIG` | user settings file |
| `GIT_TOOLS_DATA_DIR` | absolute data directory shared by the CLI, server and viewer |
| `GIT_TOOLS_BINDIR` | source or Linux package install directory |
| `RUST_LOG` | server log filter |

Set custom data and config paths before registering startup with `gtl server install`, so the
service keeps the same configuration. See [data export and import](data.md) for moving data
between machines.
