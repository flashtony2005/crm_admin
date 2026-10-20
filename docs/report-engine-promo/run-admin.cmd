@echo off
cd /d F:\project\admin\web
echo admin web on http://127.0.0.1:5188  (login: owner / demo1234)
"C:\Users\Administrator\.workbuddy\binaries\node\versions\22.22.2-6\node.exe" node_modules\vite\bin\vite.js --host 127.0.0.1 --port 5188
pause
