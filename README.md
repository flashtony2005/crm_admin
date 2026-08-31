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

## 脚本

| 脚本 | 用途 |
|---|---|
| `server/build_sections.cmd` | 重编译 cms-server（热缓存 `tgt_build4` 增量）并重启 8088 |
| `scripts/dev.ps1` | 一键开发：停旧进程 → cargo build → 起 8088 后端 + 5188 前端（`-NoBuild` 跳过编译） |
| `scripts/backup.ps1` | 备份 SQLite 到 `server/backups/`（优先 sqlite3 在线备份；`-Keep N` 保留份数，默认 30） |

## 数据库迁移

建表与补列走 `server/src/db.rs` 中 `MIGRATIONS` 版本化迁移，由 `_migrations`
表记录已应用版本，启动时判重跳过。**已发布的 version 只追加、不修改**；
新增 schema 变更在列表末尾追加新版本即可。运行库（`*.db`）不入库，
空库首次启动自动建表并写入演示种子。
