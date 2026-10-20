@echo off
REM One-click launcher: opens 3 console windows. Close a window to stop that service.
start "cms-server 8088" cmd /k "%~dp0run-backend.cmd"
timeout /t 3 /nobreak >nul
start "admin-web 5188" cmd /k "%~dp0run-admin.cmd"
start "coucouya 5199" cmd /k "%~dp0run-coucouya.cmd"
echo.
echo   admin     http://127.0.0.1:5188    owner / demo1234
echo   backend   http://127.0.0.1:8088
echo   homepage  http://127.0.0.1:5199
echo.
echo Close the three console windows to stop the services.
pause
