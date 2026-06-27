@echo off
where python >nul 2>nul
if %errorlevel%==0 (
  python "%~dp0dirlens.py" %*
) else (
  py "%~dp0dirlens.py" %*
)
