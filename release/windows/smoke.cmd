@echo off
setlocal
if "%~1"=="" exit /b 2
set "smoke_source=%~f1"
set "smoke_root=%TEMP%\git-tools-smoke-%RANDOM%-%RANDOM%"
set "smoke_bin=%smoke_root%\Local Tools"
set "GIT_TOOLS_DATA_DIR=%smoke_root%\data"
set "GIT_TOOLS_CONFIG=%smoke_root%\config.toml"
set "GIT_CONFIG_NOSYSTEM=1"
set "GIT_CONFIG_GLOBAL=%smoke_root%\empty.gitconfig"
set "GIT_TOOLS_NO_OPEN=1"
set "NO_COLOR=1"
mkdir "%smoke_root%" || exit /b 1
type nul > "%GIT_CONFIG_GLOBAL%"
call "%smoke_source%\install.cmd" "%smoke_bin%"
if errorlevel 1 goto failed
"%smoke_bin%\git-tools.exe" --version || goto failed
"%smoke_bin%\gtl.exe" --help || goto failed
"%smoke_bin%\gtl-server.exe" --version || goto failed
for /l %%i in (1,1,30) do (
    "%smoke_bin%\gtl.exe" server status >nul 2>&1 && goto ready
    ping -n 2 127.0.0.1 >nul
)
echo The installed server did not become ready within 30 seconds.
goto failed
:ready
mkdir "%smoke_root%\repository with spaces" || goto failed
cd /d "%smoke_root%\repository with spaces" || goto failed
git init -q -b main || goto failed
git config user.name "Git Tools smoke" || goto failed
git config user.email "git-tools-smoke@example.invalid" || goto failed
git config commit.gpgsign false || goto failed
git config core.autocrlf false || goto failed
echo original>sample.txt
git add sample.txt || goto failed
git commit -qm original || goto failed
echo changed>sample.txt
git commit -qam changed || goto failed
"%smoke_bin%\gtl.exe" status --color never || goto failed
"%smoke_bin%\gtl.exe" diff --raw --last 1 > "%smoke_root%\raw-url.txt" || goto failed
findstr /b "file://" "%smoke_root%\raw-url.txt" >nul || goto failed
"%smoke_bin%\gtl.exe" data export --to "%smoke_root%\snapshot" || goto failed
"%smoke_bin%\gtl.exe" data import --from "%smoke_root%\snapshot"
if not errorlevel 3 goto failed
if errorlevel 4 goto failed
cscript //nologo "%smoke_source%\setup.vbs" stop "%smoke_bin%" || goto failed
"%smoke_bin%\gtl.exe" data import --from "%smoke_root%\snapshot" || goto failed
cscript //nologo "%smoke_source%\setup.vbs" start "%smoke_bin%" || goto failed
for /l %%i in (1,1,30) do (
    "%smoke_bin%\gtl.exe" server status >nul 2>&1 && goto imported
    ping -n 2 127.0.0.1 >nul
)
echo The imported database's server did not become ready.
goto failed
:imported
set "GIT_TOOLS_NO_OPEN="
"%smoke_bin%\gtl.exe" diff --last 1 || goto failed
for /l %%i in (1,1,30) do (
    tasklist /fi "IMAGENAME eq gtl-viewer.exe" /nh | findstr /i "gtl-viewer.exe" >nul && goto viewer_ready
    ping -n 2 127.0.0.1 >nul
)
echo The installed viewer did not start within 30 seconds.
goto failed
:viewer_ready
echo Installed Windows daemon, named-pipe CLI, raw diff, safe import, and viewer smoke passed.
set "smoke_exit=0"
goto cleanup
:failed
set "smoke_exit=1"
:cleanup
cd /d "%TEMP%"
cscript //nologo "%smoke_source%\setup.vbs" uninstall "%smoke_bin%"
if errorlevel 1 exit /b 1
rmdir /s /q "%smoke_root%"
exit /b %smoke_exit%
