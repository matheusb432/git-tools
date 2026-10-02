# Diffs

Diffs open in the desktop viewer by default. Use `--raw` to save one as an offline HTML document.

## Unpushed and local changes

```bash
gtl diff # unpushed commits
# ^ is aliased by:
gtl d

gtl diff HEAD # staged, unstaged and untracked changes since HEAD
```

The default comparison uses the current branch's upstream. When there is no upstream, it shows
committed changes since the common ancestor with the project's comparison branch, initially
local `main`. Change that branch through the project's edit action in the viewer.

Local changes are a separate comparison. Use `HEAD` as above, or open Local changes from the
Projects dashboard.

## Commits and ranges

```bash
gtl diff --last # last commit
gtl diff --last 5 # last five commits
gtl diff 'HEAD~3..HEAD' # committed range
gtl diff 'HEAD^!' # this commit only
gtl diff HEAD~3 # changes since this base, including local edits
gtl diff --last --name 'my review' # label in the viewer's history
```

Quote revisions containing shell punctuation, such as `^!`.

## Merge comparisons

Show what merging the current branch into `main` would introduce:

```bash
gtl diff --merge main # main...HEAD
gtl diff merge --repo ./apps/api --base main # same comparison for a nested repository
```

## Nested repositories

```bash
gtl diff --recursive # repositories under the current directory
gtl diff --recursive --worktrees # includes nested linked worktrees
```

The scan skips nested linked worktrees unless `--worktrees` is given. Submodule checkouts count
as repositories. Each repository gets its own section in a raw document.

Use `gtl project diff --all` for [managed projects](projects.md) instead of a directory scan.

## Terminal pager

```bash
gtl diff tui # unpushed commits
gtl diff tui HEAD # staged, unstaged and untracked changes
gtl diff tui --last 5
gtl diff tui --merge main
gtl diff tui HEAD --id MY # select one managed project
```

The terminal pager shows a continuous unified diff for one repository, with syntax highlighting
from the same parser as the desktop viewer. Added and removed lines keep distinct backgrounds
and markers. Lines wrap by default;
`w` toggles wrapping and the left/right arrows scroll unwrapped lines. `f` opens the file picker,
`[` and `]` jump between hunks, and `{` and `}` jump between files. Use `/` to search and `n`/`N`
for the next/previous match. `?` opens scrollable help; `q` exits.

Wide terminals show a file sidebar with filenames, directories, and change counts. On narrow
terminals, `f` opens the file browser across the screen. Press `/` there to filter filenames.
Click a file to open it, click any toolbar control to activate it, or use the mouse wheel over
either pane. Both scrollbars support clicking and dragging. Mouse support depends on the
terminal forwarding mouse events; keyboard controls remain available. `NO_COLOR` disables colors.

`c` toggles full context. The comparison stays fixed until `r` refreshes it, retaining your
position where possible. The pager uses the local server and the saved repository extension
filter. Its session is independent of desktop tabs and history. It requires an interactive
terminal; recursive scans and managed-project batches use the existing diff commands.

## Offline HTML

```bash
gtl diff --raw # prints the file URL without opening the viewer
gtl diff HEAD --raw
gtl project diff --all --raw
```

The file contains its styles and diff content and needs no JavaScript, server or internet
connection to read. You can copy it elsewhere and open it directly in a browser.

Offline diffs always use unified layout. Theme, language, density, line wrapping and extension
filters come from the saved settings when the document is generated. File sections and long
lines expand through the browser's own controls. Use the browser's find command to search.
