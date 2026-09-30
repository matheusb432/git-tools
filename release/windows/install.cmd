@echo off
cscript //nologo "%~dp0setup.vbs" install %*
exit /b %errorlevel%
