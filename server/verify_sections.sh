#!/usr/bin/env bash
# 验证 sections 资源 + 公开 API（针对重启后的 8088 正式端口）
# 前置：已用 build_sections.cmd 完成编译并重启 cms-server
# 用法: bash verify_sections.sh
set -u
BASE="http://127.0.0.1:8088"

echo "===== 0) 服务可达性 ====="
curl -s -o /dev/null -w "HTTP %{http_code}\n" "$BASE/api/public/site" || { echo "服务不可达"; exit 1; }

echo
echo "===== 1) GET /api/public/sections (匿名) ====="
curl -s "$BASE/api/public/sections" | python -m json.tool 2>/dev/null || curl -s "$BASE/api/public/sections"

echo
echo "===== 2) 登录拿 token (owner/demo1234) ====="
TOKEN=$(curl -s -X POST "$BASE/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"owner","password":"demo1234"}' | python -c "import sys,json; d=json.load(sys.stdin); print(d.get('data',{}).get('token',''))" 2>/dev/null)
echo "token length: ${#TOKEN}"

echo
echo "===== 3) GET /api/sections (认证) ====="
curl -s -H "Authorization: Bearer $TOKEN" "$BASE/api/sections" | python -m json.tool 2>/dev/null || curl -s -H "Authorization: Bearer $TOKEN" "$BASE/api/sections"

echo
echo "===== 4) GET /api/admin/site (检查 homeTheme 字段) ====="
curl -s -H "Authorization: Bearer $TOKEN" "$BASE/api/admin/site" | python -c "import sys,json; d=json.load(sys.stdin); print('homeTheme =', d.get('data',{}).get('homeTheme'))" 2>/dev/null

echo
echo "===== 5) PUT home_theme 回写测试 (可选，验证方案A) ====="
curl -s -X PUT "$BASE/api/admin/site" -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"home_theme":"duck"}' | python -c "import sys,json; d=json.load(sys.stdin); print('put ok =', d.get('data',{}).get('homeTheme','?'))" 2>/dev/null

echo
echo "done"
