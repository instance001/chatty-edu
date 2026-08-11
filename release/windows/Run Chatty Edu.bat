@echo off
setlocal

cd /d "%~dp0"

if not exist "chatty-edu.exe" (
    echo Missing chatty-edu.exe in %~dp0
    pause
    exit /b 1
)

start "" "chatty-edu.exe"
