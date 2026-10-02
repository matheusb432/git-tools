# Install Git Tools

Keep `git-tools`, its `gtl` alias, `gtl-server`, and `gtl-viewer` together. The viewer
contains its frontend and needs no web server or internet connection to display diffs.
Git must be installed and available on PATH.

## Windows 11 x64

Install [Git for Windows](https://gitforwindows.org/), then run the `-setup.exe` installer
and open Git Tools from the Start menu. The installer starts the server and registers it
for your next login. It installs WebView2 when that runtime is missing.
The executables include the MSVC runtime and need no Visual C++ Redistributable.

Installation uses your user account and needs no administrator access. Run the new
installer to update, or uninstall through Windows Settings -> Apps. Settings and data
are preserved. To use `gtl` in a terminal, add the installation directory to your user PATH.

The installer is unsigned. Windows may ask you to confirm the publisher before running it.

Settings default to `%USERPROFILE%\.config\git-tools\config.toml`, and data lives
under `%APPDATA%\git-tools\data`. `GIT_TOOLS_CONFIG` and `GIT_TOOLS_DATA_DIR` can
select isolated settings and data; the server and its clients must use the same data root.

Before `gtl data import`, close the viewer and run `gtl server stop`. After a successful
import, run `gtl server start`. Import refuses to replace a database while its server is running.

## Ubuntu 24.04 or newer, x64

Install the `.deb` with your package installer, then open Git Tools from the application menu.
Git and the runtime libraries are installed as package dependencies. From a terminal:

```sh
sudo apt install ./git-tools-*-linux-x86_64.deb
```

The application launcher starts the systemd user service; it also starts at your next
graphical login. The commands are installed in `/usr/bin`. Install a new `.deb` to update,
or remove the application with `sudo apt remove git-tools`. Settings and data are preserved.

## macOS on Apple Silicon

Install the Apple Silicon (`arm64`) `.pkg`, then
open `/Applications/gtl-viewer.app`. The package installs the CLI in `/usr/local/bin`
and starts the server at login. Keep the app in `/Applications`.

The app is ad-hoc signed and is not notarized. Allow installation and first launch
in System Settings -> Privacy & Security when macOS blocks them.
