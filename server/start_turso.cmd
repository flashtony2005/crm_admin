@echo off
REM ============================================================
REM  Start cms-server in LOCAL TURSO mode
REM  - turso_local.py: libsql /v2/pipeline protocol server on 127.0.0.1:8081
REM    (reads/writes the SAME cms.db file, no Docker needed)
REM  - cms-server with TURSO_URL -> all queries go through Turso path
REM  Requires: cms-server.exe already built (run build_sections.cmd once)
REM ============================================================
cd /d F:/project/admin/server

netstat -ano | findstr :8081 | findstr LISTENING >nul
if errorlevel 1 (
  echo [1/2] Starting local turso protocol server on :8081 ...
  start "turso-local" /min "C:\Users\Administrator\.workbuddy\binaries\python\versions\3.13.12\python.exe" turso_local.py
  timeout /t 2 /nobreak >nul
) else (
  echo [1/2] turso server already running on :8081
)

echo [2/2] Starting cms-server on :8088 in TURSO mode ...
set "TURSO_URL=http://127.0.0.1:8081"
set "TURSO_AUTH_TOKEN=local"
start "" tgt_build4\debug\cms-server.exe
timeout /t 3 /nobreak >nul
echo.
echo DONE. Verify: curl http://127.0.0.1:8088/healthz
echo Data file: F:\project\admin\server\cms.db  (same file as before)
pause
