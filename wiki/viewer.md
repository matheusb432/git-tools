# Desktop viewer

The viewer opens projects, local changes and saved diff snapshots. Start it directly or open a
diff from a repository:

```bash
gtl-viewer
gtl diff
```

## Reviewing

Projects lists your managed repositories and their Git status. Click a row to open its committed
comparison, or open its Local changes view to review working-tree edits.

In a diff, Files lists modified files and Commits lets you select an individual commit's patch.
Use the Files controls to sort, filter or collapse the diffs. The header buttons show or hide the
two sidebars; you can also drag their boundaries to collapse or reopen them.

The tab menu offers Refresh and Live. A snapshot keeps the commit IDs it was opened with;
Refresh resolves the comparison again, and Live follows changes while you work. Saved tabs and
history remain available when you reopen the app.

Mark a file reviewed using the circle in its diff header, or press Alt+R for the current file.
Reviewed files show a check in Files, and the panel shows your progress once you mark a file.
Choose Unreviewed files only in the existing file filters to focus on the remaining work.
Marks save locally and are shared with the terminal pager.
Refresh and Live preserve marks for unchanged content and clear the displayed mark when a file's comparison changes, including binary edits.
Layout, density, wrapping and theme changes preserve progress.

## Finding commits

Open Find commits from a project row, or the search icon beside the Commits heading. Search by
message or hash, with optional From and Until bounds in local time.

The Commits search starts with This snapshot. Switch to Active branch to include pushed commits
reachable from the checked-out branch. Project-row search uses that branch too. Neither search
fetches remotes.

## Pushing

Push from the Commits heading, the tab menu, the review dock or a project row.

The viewer pushes the selected commit, or the newest displayed commit when none is selected.
Projects pushes the current branch tip. Confirmation shows the destination, commit count and
SHA; expand Details & command to inspect the exact command.

Viewer pushes allow dirty files and send existing commits to the configured upstream. They
leave local edits untouched.

## Settings and shortcuts

Open the gear button for theme, layout, density, line wrapping, language and other preferences.
The viewer and generated HTML support English (US) and Brazilian Portuguese. Settings save as
you change them; project edits use an explicit Save.

Under Keyboard Shortcuts you can search, record a binding, remove it or reset it. Defaults
include Ctrl+P for files, Ctrl+F for text filters, Ctrl+Tab and Ctrl+Shift+Tab for diff tabs, and
Ctrl+Enter for pushing the active diff. On macOS, search and Push use Command; tab cycling
keeps Control.

Under Git, CLI and viewer push confirmation are independent and both enabled by default.
You can disable viewer confirmation globally or for one project through its edit action.
Skipping confirmation still uses the same server checks.

Use the `config.toml` link in Settings to open the saved file for manual edits. See
[installation and updates](installation-and-update.md#data-and-config) for its path and
[troubleshooting](troubleshooting.md) for invalid settings or connection failures.
