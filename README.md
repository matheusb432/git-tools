# git-tools

CLI and desktop app to review diffs and run repetitive Git commands across your projects.

This runs locally. The viewer works offline, and generated diffs can be saved as a single HTML
file to open in a browser or share. Commands that fetch or push need access to your Git remotes.

Examples use `gtl`, the short form of `git-tools`.

## Getting started

Open a diff from your repository:

```bash
cd ~/code/my-app
gtl diff # unpushed commits
gtl diff HEAD # staged, unstaged and untracked changes
```

## References

- [Installation and updates](wiki/installation-and-update.md)
- [Projects](wiki/projects.md)
- [Diffs](wiki/diffs.md)
- [Desktop viewer](wiki/viewer.md)
- [Git workflows](wiki/git-workflows.md)
- [Data export and import](wiki/data.md)
- [Troubleshooting](wiki/troubleshooting.md)
- [Design](wiki/design.md)
