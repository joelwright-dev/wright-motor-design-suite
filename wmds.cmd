@echo off
REM Run the WMDS command line without worrying about PATH. Examples:
REM
REM     wmds check vehicles\reference-city-ev\reference-city-ev.veh.kdl
REM     wmds veh show vehicles\reference-city-ev\reference-city-ev.veh.kdl --build
REM     wmds chassis show mcds-v1 --config full-length --width wide
REM     wmds lib validate
REM
REM Everything after the script name is passed straight through.

setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where cargo >nul 2>nul
if errorlevel 1 (
    echo Could not find cargo. Install Rust from https://rustup.rs and run this again.
    exit /b 1
)

cargo run -q -p wmds-cli -- %*
