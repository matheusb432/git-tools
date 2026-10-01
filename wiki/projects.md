# Projects

A managed project connects a short ID and title to a repository directory. This lets you select
it from anywhere or run commands across your active projects.

## Creating

Register a project with an absolute repository path:

```bash
gtl project add '{"project_id":"APP","title":"my-app","source":{"kind":"directory","path":"/home/you/code/my-app"}}'
```

IDs contain two to four ASCII letters and are stored in uppercase. CLI selection is
case-insensitive, so `app` and `APP` select the same project.

In the [viewer](viewer.md), open Projects → Add and select a directory to scan for repositories.
Select the ones you want to add. The scan includes nested repositories and submodule checkouts;
repositories already managed are shown but cannot be added again.

## Listing and selecting

```bash
gtl project ls # active projects and their Git status
# ^ is aliased by:
gtl ls

gtl diff --id app # opens this project's diff, regardless of the current directory
gtl pull --id app
gtl push --id app
```

Use `--json` on `project ls` when a script needs structured output.

## Pausing and resuming

```bash
gtl project pause app
gtl project resume app
```

Pausing keeps the project and its diff history, but leaves it out of active-project batches.
You can still select it with `--id` or open it in the viewer. These commands leave repository
files unchanged.

The Projects dashboard shows Active projects by default. Change the filter to Paused or All to
find a paused project. Each row has pause/resume and edit actions; editing opens the comparison
branch, viewer push confirmation and snapshot history.

## Working across projects

```bash
gtl project diff --all # diffs for active projects with committed changes to review
gtl project pull --all --dry # previews fast-forwards; still fetches from origin
gtl project pull --all
gtl project push --all --dry # previews pushes
gtl project push --all
```

Batch pushes use existing commits. They also respect each project's `excluded_from_push_all`
setting in [config.toml](../config/local/config.example.toml).

See [diffs](diffs.md) for comparisons and [Git workflows](git-workflows.md) for push and pull
behavior.
