#!/usr/bin/env bash
# =====================================================================
# coucouya CMS — 一键部署脚本（Linux / macOS）
# ---------------------------------------------------------------------
# 流程：
#   1) 工具预检（cargo / node / pnpm|npm / python3）
#   2) 加载 .env 并运行配置检查门禁（scripts/check_config.py --strict）
#   3) 构建后端  cargo build --release
#   4) 构建前端  pnpm build  -> web/dist
#   5) 组装产物  deploy/  （二进制 + .env + dist-app + start.sh + nginx + systemd）
#
# 用法：
#   ./scripts/deploy.sh                 # 默认严格模式
#   STRICT=0 ./scripts/deploy.sh        # 放宽：WARN 不阻断
#   ENV_FILE=prod.env ./scripts/deploy.sh
#
# 运行：
#   cd deploy && ./start.sh             # 直接运行（监听 $PORT，默认 8088）
#   或参考 deploy/cms-server.service 用 systemd 托管
# =====================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ENV_FILE="${ENV_FILE:-.env}"
STRICT="${STRICT:-1}"

echo "==> [1/5] 工具预检"
command -v cargo >/dev/null 2>&1 || { echo "✗ 需要 Rust 工具链 (cargo)"; exit 1; }
command -v node  >/dev/null 2>&1 || { echo "✗ 需要 Node.js"; exit 1; }
PKG="$(command -v pnpm || command -v npm)"
[ -n "$PKG" ] || { echo "✗ 需要 pnpm 或 npm"; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "✗ 需要 python3（用于配置检查）"; exit 1; }

echo "==> [2/5] 加载 .env 并运行配置检查门禁"
if [ -f "$ENV_FILE" ]; then
  set -a; . "./$ENV_FILE"; set +a
  echo "    已加载 $ENV_FILE"
else
  echo "    ⚠ 未找到 $ENV_FILE，将用内置默认值（生产请先 cp .env.example .env）"
fi

if ! python3 "$ROOT/scripts/check_config.py" --env-file "$ENV_FILE" ${STRICT:+"--strict"}; then
  echo "✗ 配置检查未通过，已中止部署。请按上文修复后重试。"
  exit 1
fi

echo "==> [3/5] 构建后端 (cargo build --release)"
( cd server && cargo build --release )

echo "==> [4/5] 构建前端 ($PKG build)"
( cd web && "$PKG" install --prefer-offline && "$PKG" run build )

echo "==> [5/5] 组装部署产物 -> deploy/"
rm -rf deploy && mkdir -p deploy/dist-app
# 复制后端二进制（兼容 Windows 交叉产物名）
cp server/target/release/cms-server    deploy/ 2>/dev/null || \
cp server/target/release/cms-server.exe deploy/ 2>/dev/null || \
  { echo "✗ 未找到编译产物"; exit 1; }
cp -r web/dist/. deploy/dist-app/
[ -f "$ENV_FILE" ] && cp "$ENV_FILE" deploy/.env

# 生成 start.sh
cat > deploy/start.sh <<'SH'
#!/usr/bin/env bash
set -a; [ -f "$(dirname "$0")/.env" ] && . "$(dirname "$0")/.env"; set +a
cd "$(dirname "$0")"
exec ./cms-server
SH
chmod +x deploy/start.sh

# 生成 nginx 反代示例（生产推荐：HTTPS + 静态直出）
cat > deploy/nginx.conf.example <<'NGINX'
server {
    listen 80;
    server_name cms.example.com;           # 改成你的域名
    return 301 https://$host$request_uri;  # 强制 HTTPS
}
server {
    listen 443 ssl;
    server_name cms.example.com;

    ssl_certificate     /etc/letsencrypt/live/cms.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/cms.example.com/privkey.pem;

    client_max_body_size 20m;

    # 静态资源直出（比反代更快）
    location /assets/ { root /opt/coucouya/deploy/dist-app; expires 7d; }
    location = /favicon.ico { root /opt/coucouya/deploy/dist-app; }
    location = /robots.txt  { root /opt/coucouya/deploy/dist-app; }

    # 后端 API / 上传
    location /api/    { proxy_pass http://127.0.0.1:8088; proxy_set_header Host $host; proxy_set_header X-Real-IP $remote_addr; }
    location /uploads/ { proxy_pass http://127.0.0.1:8088; }
    location /graphql { proxy_pass http://127.0.0.1:8088; }
    location /sitemap.xml { proxy_pass http://127.0.0.1:8088; }
    location /rss.xml     { proxy_pass http://127.0.0.1:8088; }
    location /robots.txt  { proxy_pass http://127.0.0.1:8088; }

    # SPA history 回退（其余路径交由 cms-server 托管也可）
    location / {
        proxy_pass http://127.0.0.1:8088;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
NGINX

# 生成 systemd 单元
cat > deploy/cms-server.service <<'UNIT'
[Unit]
Description=coucouya CMS server
After=network.target

[Service]
WorkingDirectory=/opt/coucouya/deploy
EnvironmentFile=/opt/coucouya/deploy/.env
ExecStart=/opt/coucouya/deploy/cms-server
Restart=on-failure
User=cms

[Install]
WantedBy=multi-user.target
UNIT

echo
echo "✅ 部署产物已生成于 deploy/"
echo "   运行：      cd deploy && ./start.sh"
echo "   或 systemd： sudo cp deploy/cms-server.service /etc/systemd/system/ && sudo systemctl enable --now cms-server"
echo "   默认监听 \$PORT (8088)；站点根托管 web/dist，/api 提供后端 API。"
echo "   生产建议用 Nginx 反代（deploy/nginx.conf.example）+ HTTPS。"
