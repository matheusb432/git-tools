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
