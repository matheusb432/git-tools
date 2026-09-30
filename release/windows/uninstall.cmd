@echo off
cscript //nologo "%~dp0setup.vbs" uninstall %*
exit /b %errorlevel%
