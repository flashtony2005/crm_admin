@echo off
REM ============================================================
REM  Clear login lockouts  (table: login_locks)
REM  Use when login says: too many attempts, retry in 15 minutes
REM  Only removes lock/counter rows - no user data is touched.
REM  Accounts: owner / editor / viewer      Password: demo1234
REM ============================================================
set PY=C:\Users\Administrator\.workbuddy\binaries\python\versions\3.13.12\python.exe
cd /d F:\project\admin\server
"%PY%" unlock_login.py
echo.
pause
