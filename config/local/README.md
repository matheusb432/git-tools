# Local config link

`config.toml` is an ignored, repo-only symlink to the global git-tools config at `~/.config/git-tools/config.toml`; the CLI does not load config from this folder.

Recreate it with `ln -s "$HOME/.config/git-tools/config.toml" config/local/config.toml`.
