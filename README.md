# crm_admin

面向现代在线发布的轻量内容管理后台。专注内容创作体验，自托管优先。

## 技术栈

- **后端** `server/`：Rust + Axum，数据层使用 SQLite（sea-orm / sqlx）。服务名 `cms-server`，默认端口 **8088**。
- **前端** `web/`：React + Vite + HeroUI v3 + TanStack Router。开发端口 **5188**，通过 Vite 代理把 `/api` 转发到后端 8088。
- **文档** `docs/`：项目说明文档。

## 目录结构

```
server/   后端源码（Rust / Axum）
web/      前端源码（React / Vite）
docs/     文档
```

## 本地运行

```bash
# 后端
cd server
cargo build            # 或 --release
./target/debug/cms-server.exe   # 配套 cms_live.db

# 前端
cd web
pnpm install
pnpm dev                # http://localhost:5188
```

> 前端经 Vite 代理访问 `http://127.0.0.1:8088/api`，请先启动后端。
