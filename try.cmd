@echo off
REM Run everything WMDS can do on a vehicle and open the results.
REM
REM     try.cmd                                  the reference vehicle
REM     try.cmd vehicles\mine\mine.veh.kdl       any other one
REM
REM Writes reports\ and then opens the editor.

setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

set "VEH=%~1"
if "%VEH%"=="" set "VEH=vehicles\reference-city-ev\reference-city-ev.veh.kdl"

where cargo >nul 2>nul
if errorlevel 1 (
    echo Could not find cargo. Install Rust from https://rustup.rs and run this again.
    pause
    exit /b 1
)

if not exist reports mkdir reports

echo Building. The first run takes a few minutes; after that it is seconds.
cargo build -q --release -p wmds-cli --no-default-features
if errorlevel 1 goto :failed
cargo build -q -p wmds-app
if errorlevel 1 goto :failed

echo.
echo === What it is made of, and how to put it together ===
cargo run -q --release -p wmds-cli --no-default-features -- build "%VEH%" --volume 500 --markdown reports\build-pack.md > reports\build-pack.txt
type reports\build-pack.txt | more

echo.
echo === How it drives ===
cargo run -q --release -p wmds-cli --no-default-features -- drive "%VEH%" > reports\handling.txt
type reports\handling.txt

echo.
echo === Whether it complies ===
cargo run -q --release -p wmds-cli --no-default-features -- check "%VEH%" > reports\compliance.txt
type reports\compliance.txt

echo.
echo Reports written to the reports folder. Opening the editor.
start "" cargo run -q -p wmds-app -- "%VEH%"
exit /b 0

:failed
echo.
echo The build failed. The output above says why.
pause
exit /b 1
