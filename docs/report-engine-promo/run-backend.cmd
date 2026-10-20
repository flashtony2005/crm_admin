@echo off
cd /d F:\project\admin\server
set PORT=8088
if not exist tgt_build4\debug\cms-server.exe (
  echo [ERROR] cms-server.exe not found: %CD%\tgt_build4\debug\cms-server.exe
  pause
  exit /b 1
)
echo cms-server on http://127.0.0.1:8088  (DB: %CD%\cms.db)
tgt_build4\debug\cms-server.exe
pause
