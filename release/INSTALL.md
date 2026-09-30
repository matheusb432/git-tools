# Install Git Tools

Keep `git-tools`, its `gtl` alias, `gtl-server`, and `gtl-viewer` together. The viewer
contains its frontend and needs no web server or internet connection to display diffs.
Git must be installed and available on PATH.

## Windows 11 x64

Install [Git for Windows](https://gitforwindows.org/) and Microsoft's
[WebView2 Evergreen Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).
WebView2 is normally present on Windows 11; LTSC installations can require it separately.
The executables include the MSVC runtime and need no Visual C++ Redistributable.

Extract the ZIP before running `install.cmd` as your normal user. It copies all
executables to `%LOCALAPPDATA%\Programs\git-tools`, creates a Start menu shortcut,
and starts the server without a console window. A Startup shortcut starts the server
at your next login. This uses Windows Script Host and requires no administrator account.
You can delete the extracted directory after installation.

Add `%LOCALAPPDATA%\Programs\git-tools` to your user PATH in Windows' Environment
Variables settings, then sign out and back in. Open Git Tools from the Start menu or
run `gtl-viewer.exe`. From a repository, run `gtl status` or `gtl diff --last 1`.

For a different directory, run `install.cmd "C:\Users\you\Local Tools\git-tools"`
from Command Prompt. Use an absolute path owned by your user. Updating through
`install.cmd` stops that directory's running server and viewer before replacing them.
Run the installed `uninstall.cmd` to remove the executables and login shortcuts;
settings and data are preserved. Pass the same custom directory when uninstalling.

Without installation, keep the extracted files together, run `gtl-server.exe` in a
terminal, and open `gtl-viewer.exe` or the CLI from another terminal. Ctrl+C stops the
server. All native clients use a private named pipe for the current user.

Settings default to `%USERPROFILE%\.config\git-tools\config.toml`, and data lives
under `%APPDATA%\git-tools\data`. `GIT_TOOLS_CONFIG` and `GIT_TOOLS_DATA_DIR` can
select isolated settings and data; the server and its clients must use the same data root.

Before `gtl data import`, close the viewer and stop the installed server with
`cscript //nologo "%LOCALAPPDATA%\Programs\git-tools\setup.vbs" stop`. After a successful
import, use the same command with `start`. Pass the custom install directory as the second
argument when applicable. Import refuses to replace a database while its server is running.

## Ubuntu 24.04 x64

Install the runtime libraries:

```sh
sudo apt-get install git libwebkit2gtk-4.1-0 libgtk-3-0t64 \
  libayatana-appindicator3-1 libxdo3
```

Extract the `.tar.gz` and run `./install.sh` in a graphical login session. It installs
all executables to `~/.local/bin` and enables the systemd user service. Add that
directory to PATH. `GIT_TOOLS_BINDIR` can select another absolute install directory.

Run `gtl-viewer` to open the desktop app or `gtl diff --last 1` from a repository.
Inspect the daemon with `gtl server status` and `systemctl --user status gtl-server`.

## macOS

Choose the `.pkg` for Apple Silicon (`arm64`) or Intel (`x86_64`). Install it, then
open `/Applications/gtl-viewer.app`. The package installs the CLI in `/usr/local/bin`
and starts the server at login. Keep the app in `/Applications`.

The app is ad-hoc signed and is not notarized. Allow installation and first launch
in System Settings -> Privacy & Security when macOS blocks them.
