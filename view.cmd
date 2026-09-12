@echo off
REM Open the WMDS viewer. Double-click this file, or run it with a path to open something else:
REM     view.cmd library\suspension\arms\lca-wishbone-a.prim.kdl
REM
REM Exists because a terminal opened before Rust was installed does not have cargo on its PATH,
REM and that has cost more time than it should have.

setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

set "TARGET=%~1"
if "%TARGET%"=="" set "TARGET=vehicles\reference-city-ev\reference-city-ev.veh.kdl"

where cargo >nul 2>nul
if errorlevel 1 (
    echo Could not find cargo. Install Rust from https://rustup.rs and run this again.
    pause
    exit /b 1
)

echo Building the viewer. The first run after a change takes a minute; after that it is instant.
cargo build -q -p wmds-app
if errorlevel 1 (
    echo.
    echo The build failed. The output above says why.
    pause
    exit /b 1
)

echo Opening %TARGET%
cargo run -q -p wmds-app -- "%TARGET%"
if errorlevel 1 pause
