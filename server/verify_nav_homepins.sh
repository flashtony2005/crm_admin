#!/usr/bin/env bash
# 验证 nav_links / home_pins 资源 + 公开 API（针对重启后的 8088 正式端口）
# 前置：已用 build_sections.cmd 完成编译并重启 cms-server
# 用法: bash verify_nav_homepins.sh
set -u
BASE="http://127.0.0.1:8088"

echo "===== 0) 服务可达性 ====="
CODE=$(curl -s -m 5 -o /dev/null -w "%{http_code}" "$BASE/api/public/site")
echo "HTTP $CODE"
[ "$CODE" = "200" ] || { echo "服务不可达"; exit 1; }

echo
echo "===== 1) GET /api/public/nav (匿名, 期望 nav/footer 两组) ====="
curl -s "$BASE/api/public/nav" | python -m json.tool 2>/dev/null || curl -s "$BASE/api/public/nav"

echo
echo "===== 2) GET /api/public/home-pins?slot=writing (匿名) ====="
curl -s "$BASE/api/public/home-pins?slot=writing" | python -m json.tool 2>/dev/null || curl -s "$BASE/api/public/home-pins?slot=writing"

echo
echo "===== 3) GET /api/public/articles?featured=1&limit=3 (匿名, 验证 ?limit=) ====="
curl -s "$BASE/api/public/articles?featured=1&limit=3" | python -c "import sys,json; d=json.load(sys.stdin); print('count =', len(d.get('data',[])))" 2>/dev/null

echo
echo "===== 4) 登录拿 token (owner/demo1234) ====="
TOKEN=$(curl -s -X POST "$BASE/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"owner","password":"demo1234"}' | python -c "import sys,json; d=json.load(sys.stdin); print(d.get('data',{}).get('token',''))" 2>/dev/null)
echo "token length: ${#TOKEN}"

echo
echo "===== 5) GET /api/navlinks (认证, 期望迁移后的 footer 3 条) ====="
curl -s -H "Authorization: Bearer $TOKEN" "$BASE/api/navlinks" | python -m json.tool 2>/dev/null || curl -s -H "Authorization: Bearer $TOKEN" "$BASE/api/navlinks"

echo
echo "===== 6) GET /api/home_pins (认证) ====="
curl -s -H "Authorization: Bearer $TOKEN" "$BASE/api/home_pins" | python -m json.tool 2>/dev/null || curl -s -H "Authorization: Bearer $TOKEN" "$BASE/api/home_pins"

echo
echo "===== 7) POST /api/navlinks 增改删回路 (认证, 验证 CRUD) ====="
NEW_ID=$(curl -s -X POST "$BASE/api/navlinks" -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"grp":"nav","label":"验证项","href":"/tag/test","target":"","sort":99,"enabled":true}' \
  | python -c "import sys,json; d=json.load(sys.stdin); print(d.get('data',{}).get('id',''))" 2>/dev/null)
echo "created id: $NEW_ID"
if [ -n "$NEW_ID" ]; then
  curl -s -X PUT "$BASE/api/navlinks/$NEW_ID" -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
    -d '{"label":"验证项改"}' | python -c "import sys,json; d=json.load(sys.stdin); print('updated label =', d.get('data',{}).get('label'))" 2>/dev/null
  curl -s -X DELETE "$BASE/api/navlinks/$NEW_ID" -H "Authorization: Bearer $TOKEN" \
    | python -c "import sys,json; d=json.load(sys.stdin); print('deleted ok =', d.get('ok'))" 2>/dev/null
fi

echo
echo "===== 8) POST /api/home_pins 置顶回路 (认证, 取第一篇已发布文章) ====="
ART_ID=$(curl -s "$BASE/api/public/articles?limit=1" | python -c "import sys,json; d=json.load(sys.stdin); a=d.get('data',[]); print(a[0]['id'] if a else '')" 2>/dev/null)
echo "article id: $ART_ID"
if [ -n "$ART_ID" ]; then
  PIN_ID=$(curl -s -X POST "$BASE/api/home_pins" -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
    -d "{\"slot\":\"writing\",\"articleId\":\"$ART_ID\",\"sort\":99,\"enabled\":true}" \
    | python -c "import sys,json; d=json.load(sys.stdin); print(d.get('data',{}).get('id',''))" 2>/dev/null)
  echo "pin id: $PIN_ID"
  if [ -n "$PIN_ID" ]; then
    curl -s -X PUT "$BASE/api/home_pins/$PIN_ID" -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
      -d '{"enabled":false}' | python -c "import sys,json; d=json.load(sys.stdin); print('toggled enabled =', d.get('data',{}).get('enabled'))" 2>/dev/null
    curl -s -X DELETE "$BASE/api/home_pins/$PIN_ID" -H "Authorization: Bearer $TOKEN" \
      | python -c "import sys,json; d=json.load(sys.stdin); print('deleted ok =', d.get('ok'))" 2>/dev/null
  fi
fi

echo
echo "done"
