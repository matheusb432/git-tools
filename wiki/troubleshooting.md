# Troubleshooting

Run the diagnosis and follow the recovery action it prints:

```bash
gtl doctor
```

This checks the installation, service registration, settings, Git, database integrity and local
server health. Use `gtl doctor --json` for a machine-readable report.

## Server

```bash
gtl server status # registration, health and client/server versions
gtl server install # registers login startup and starts the sibling server
gtl server restart
gtl server stop
gtl server start
```

`gtl server uninstall` stops the server and removes its startup registration, keeping binaries,
settings and data. Login startup uses a user systemd service on Linux, a launch agent on macOS,
or a scheduled task on Windows. Windows ZIP packages instead install Startup shortcuts; use
the [package instructions](../release/INSTALL.md) for that installation.

The CLI, server and viewer need matching versions and the same data directory. If you've set
`GIT_TOOLS_DATA_DIR` or `GIT_TOOLS_CONFIG`, register the server with those values through
`gtl server install`. For a source checkout, `just doctor` reports missing development tools;
`gtl doctor` diagnoses the installed app.

## Viewer and settings

The viewer can open while the server is unavailable and offers reconnect. If it reports a
version mismatch, update the installation and restart the server.

Invalid settings show the config path and diagnostic. Repair the file and use Retry, or reset
it through the recovery screen. Reset saves a backup of the original file before writing
defaults.

Use command help for the current diagnostic options and server entrypoints:

```bash
gtl doctor --help
gtl server --help
```
