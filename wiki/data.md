# Data export and import

Export a snapshot to back up git-tools data or move it to another machine:

```bash
gtl data export --to ~/gtl-snapshot
```

The destination must be a new directory and its parent must exist. Export creates a consistent
database snapshot and includes `config.toml` when present, plus a manifest with the snapshot's
schema version. The server can keep running during export.

The snapshot contains application data, not your Git repositories. Project source paths stay
as saved; the receiving machine needs the repositories at those paths.

## Importing

```bash
gtl data import --from ~/gtl-snapshot
```

> [!CAUTION]
> Import replaces the current database and restores the snapshot's configuration when included.
> Export your current data first if you want to keep it.

Import stages and checks the database, migrates an older schema forward, then stops the
registered server for the replacement. After a successful import it restarts the server if it
was running. A snapshot newer than this installation's supported schema is refused.

Close the viewer before importing. If you started `gtl-server` manually, stop it too; import
refuses the replacement while a server still owns the local endpoint. Windows ZIP installs use
their own startup scripts, so follow the stop/start steps in the
[package instructions](../release/INSTALL.md#windows-11-x64).
