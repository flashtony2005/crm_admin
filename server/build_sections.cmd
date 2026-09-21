@echo off
REM ============================================================
REM  Rebuild cms-server and restart :8088
REM  Includes: nav_links + home_pins resources, home_template /
REM  main_port settings, and the templates module (/t/* static
REM  hosting, /api/public/templates, upload & activate).
REM  - Incremental build into warm tgt_build4 (a few minutes)
REM  - Stops the running dev server first (Windows cannot
REM    overwrite a running exe), restarts it right after.
REM  - Auto-removes the stale 0-byte .cargo-build-lock left by an
REM    interrupted build (otherwise cargo fails with os error 5).
REM  - This file is pure ASCII with CRLF line endings on purpose:
REM    cmd.exe mangles UTF-8 / LF-only batch files.
REM  Run this file from YOUR OWN terminal (double-click is OK).
REM ============================================================

set MSVC=C:/Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.43.34808
set SDK=C:/Program Files (x86)\Windows Kits\10
set SDKVER=10.0.26100.0

set "INCLUDE=%MSVC%\include;%SDK%\Include\%SDKVER%\ucrt;%SDK%\Include\%SDKVER%\um;%SDK%\Include\%SDKVER%\shared;%SDK%\Include\%SDKVER%\winrt;%SDK%\Include\%SDKVER%\cppwinrt"
set "LIB=%MSVC%\lib\x64;%SDK%\Lib\%SDKVER%\ucrt\x64;%SDK%\Lib\%SDKVER%\um\x64"
set "VCToolsInstallDir=%MSVC%"
set "WindowsSdkDir=%SDK%"
set "UCRTVersion=%SDKVER%"
set CARGO_TARGET_DIR=F:/project/admin/server/tgt_build4

cd /d F:/project/admin/server

REM Auto-clean the stale 0-byte lock file (left by an interrupted build)
if exist "tgt_build4\debug\.cargo-build-lock" (
  echo [0/3] Removing stale build lock ...
  del /q "tgt_build4\debug\.cargo-build-lock"
)

echo [1/3] Stopping old cms-server on :8088 ...
taskkill /F /IM cms-server.exe 2>nul
timeout /t 2 /nobreak >nul

echo [2/3] Building (incremental, a few minutes) ...
cargo build
if errorlevel 1 (
  echo.
  echo BUILD FAILED - nothing was restarted. Check errors above.
  pause
  exit /b 1
)

echo [3/3] Starting new cms-server on :8088 ...
if exist "turso_mode.flag" (
  echo       TURSO mode enabled ^(turso_mode.flag^): queries via 127.0.0.1:8081
  netstat -ano | findstr :8081 | findstr LISTENING >nul
  if errorlevel 1 (
    start "turso-local" /min "C:\Users\Administrator\.workbuddy\binaries\python\versions\3.13.12\python.exe" turso_local.py
    timeout /t 2 /nobreak >nul
  )
  set "TURSO_URL=http://127.0.0.1:8081"
  set "TURSO_AUTH_TOKEN=local"
) else (
  echo       plain SQLite mode ^(no turso_mode.flag^)
)
start "" tgt_build4\debug\cms-server.exe
timeout /t 3 /nobreak >nul
echo.
echo DONE. New API is live:
echo   GET http://127.0.0.1:8088/api/public/nav          (anonymous)
echo   GET http://127.0.0.1:8088/api/public/home-pins    (anonymous)
echo   GET http://127.0.0.1:8088/api/public/sections     (anonymous)
echo   GET http://127.0.0.1:8088/api/public/site         (anonymous, homeTemplate / mainPort)
echo   GET http://127.0.0.1:8088/api/public/templates    (anonymous, template list)
echo   GET http://127.0.0.1:8088/api/admin/templates     (needs token, manage)
echo   GET http://127.0.0.1:8088/t/coucouya/             (template static hosting)
echo   GET http://127.0.0.1:8088/t/active/               (currently active template)
echo   GET http://127.0.0.1:8088/api/navlinks            (needs token)
echo   NOTE: zip dependency is already in Cargo.toml
echo.
pause
