@echo off
REM ============================================================
REM  Rebuild cms-server and restart :8088
REM  Includes: nav_links + home_pins resources, home_template / main_port 设置
REM  - Incremental build into warm tgt_build4 (a few minutes)
REM  - Stops the running dev server first (Windows cannot
REM    overwrite a running exe), restarts it right after.
REM  Run this file from YOUR OWN terminal (double-click is OK).
REM  If the previous build was interrupted, delete the stale
REM  0-byte tgt_build4\debug\.cargo-build-lock first, otherwise
REM  cargo fails with os error 5 (access denied).
REM ============================================================

set MSVC=C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.43.34808
set SDK=C:\Program Files (x86)\Windows Kits\10
set SDKVER=10.0.26100.0

set "INCLUDE=%MSVC%\include;%SDK%\Include\%SDKVER%\ucrt;%SDK%\Include\%SDKVER%\um;%SDK%\Include\%SDKVER%\shared;%SDK%\Include\%SDKVER%\winrt;%SDK%\Include\%SDKVER%\cppwinrt"
set "LIB=%MSVC%\lib\x64;%SDK%\Lib\%SDKVER%\ucrt\x64;%SDK%\Lib\%SDKVER%\um\x64"
set "VCToolsInstallDir=%MSVC%"
set "WindowsSdkDir=%SDK%"
set "UCRTVersion=%SDKVER%"
set CARGO_TARGET_DIR=F:\project\admin\server\tgt_build4

cd /d F:\project\admin\server

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

echo [3/3] Starting new cms-server on :8088 (default .\cms.db) ...
start "" tgt_build4\debug\cms-server.exe
timeout /t 3 /nobreak >nul
echo.
echo DONE. New API is live:
echo   GET http://127.0.0.1:8088/api/public/nav          (anonymous)
echo   GET http://127.0.0.1:8088/api/public/home-pins    (anonymous)
echo   GET http://127.0.0.1:8088/api/public/sections     (anonymous)
echo   GET http://127.0.0.1:8088/api/public/site         (anonymous, +
echo        homeTemplate / mainPort 首页模板与主端口配置)
echo   GET http://127.0.0.1:8088/api/public/templates    (anonymous, 模板列表)
echo   GET http://127.0.0.1:8088/api/admin/templates     (needs token, 管理)
echo   GET http://127.0.0.1:8088/t/coucouya/             (模板静态托管)
echo   GET http://127.0.0.1:8088/t/active/               (当前激活模板)
echo   GET http://127.0.0.1:8088/api/navlinks            (needs token)
echo   NOTE: 上传模板解包需 zip 依赖（已加入 Cargo.toml）
echo.
pause
