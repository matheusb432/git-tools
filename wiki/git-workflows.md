# Git workflows

Run these from a repository. `diff`, `push` and `pull` also accept `--id` to select a
[managed project](projects.md).

## Status

```bash
gtl status
# ^ is aliased by:
gtl s

gtl status --recursive # this repository and nested repositories
gtl status --json # structured output for scripts
```

Recursive status skips nested linked worktrees. Use `gtl project ls` for status across active
managed projects.

## Pushing

```bash
gtl push # push existing commits
# ^ is aliased by:
gtl p

gtl push 'my commit message' # stages all changes, commits and pushes
gtl push --recursive # pushes existing commits in this repository and nested repositories
```

> [!NOTE]
> Supplying a message stages all changes, including unstaged and untracked files. Without a
> message, the single-repository CLI push requires a clean working tree.

The branch needs an upstream. Push asks for confirmation by default; `--yes` skips the prompt.
The CLI setting is independent of [viewer push confirmation](viewer.md#settings-and-shortcuts).

Recursive and managed-project pushes use existing commits. Commit-with-message belongs to the
single-repository command.

## Pulling

```bash
gtl pull --dry # fetches from origin and previews the fast-forward
gtl pull
```

Pull fetches `origin` and fast-forwards the current branch from the branch with the same name
there. A diverged branch needs manual resolution. `--dry` still fetches, but leaves the local
branch in place.

See [projects](projects.md#working-across-projects) for batch pushes and pulls.

## Tags

```bash
gtl tag # local tags and annotation summaries
gtl tag --commits # show the commit each tag points at
gtl tag --state # query origin and mark local/remote state
gtl tag add v1.2.3 'my release' # creates an annotated tag
gtl tag push v1.2.4 'next release' # creates and pushes this tag
```

Preview or create the next tag using a configured pattern:

```bash
gtl tag bump 'my release' --level patch --dry
gtl tag bump 'my release' --level patch
gtl tag bump 'my release' --level minor --push # creates and pushes only the new tag
```

Patterns and the default pattern come from `[tags]` in
[config.toml](../config/local/config.example.toml), with optional overrides by project name.
Use `--pattern` to select one. Without `--level`, bump increments the rightmost numeric slot.
