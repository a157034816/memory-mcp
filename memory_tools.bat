@echo off
setlocal

rem NOTE: This bat only launches Python. All user-facing messages are printed by Python.
set PYTHONUTF8=1

python -X utf8 "%~dp0tools\memory_tools.py"
exit /b %ERRORLEVEL%
