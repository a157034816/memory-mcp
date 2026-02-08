@echo off
setlocal

rem 说明：该 bat 仅负责启动 Python；所有提示与输出均由 Python 脚本打印。
set PYTHONUTF8=1

python -X utf8 "%~dp0tools\version_tools.py" %*
exit /b %ERRORLEVEL%

