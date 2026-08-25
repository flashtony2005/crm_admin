# 运行与部署

---

## 1. 环境要求

- 后端：Rust 工具链（≥ 1.75，已验证 1.95）+ Cargo。
- 前端：Node.js（≥ 18）+ npm。
- 数据库：默认本地 SQLite（无需额外服务）；可选 Turso 远程。

---

## 2. 本地开发（推荐 mock 模式上手）

```bash
# 后端（终端 A）
cd server
cargo run                      # 监听 0.0.0.0:8088（可用 PORT 环境变量覆盖）

# 前端（终端 B，默认 mock 模式）
cd web
npm install
npm run dev                   # http://localhost:5188
```

- `mock` 模式：前端用 localStorage，无需后端即可演示全部 UI。
- 默认账号（种子）：`owner` / `editor` / `viewer`，密码均为 `demo1234`。

---

## 3. 真实后端联调（real 模式）

```bash
cd web
VITE_CMS_MODE=real npm run dev
```

- `vite.config.ts` 已配置 `/api` 代理到 `http://localhost:8088`，无需手动 CORS。
- 后端 `main.rs` 使用 `CorsLayer::very_permissive()`，跨域也兼容。
- 页面代码在 mock/real 间**零改动**，仅切换 `VITE_CMS_MODE`。

---

## 4. 构建与测试

```bash
# 后端
cd server
cargo build --release         # 产物：target/release/cms-server
cargo test                    # 单元 + 集成测试

# 前端
cd web
npm run build                 # tsc -b && vite build → dist/
npm test                      # Vitest
npm run test:e2e              # Playwright（需 npx playwright install）
```

---

## 5. 后端环境变量

| 变量 | 作用 | 默认 |
|------|------|------|
| `PORT` | 监听端口 | `8088` |
| `DATABASE_URL` | 本地 SQLite 路径 | `sqlite:./cms.db?mode=rwc` |
| `TURSO_URL` | 设置则启用 Turso 远程（libsql:// 或 https://） | 未设置 |
| `TURSO_AUTH_TOKEN` | Turso 令牌（设 `TURSO_URL` 时必填） | — |
| `JWT_SECRET` | JWT 签名密钥 | `dev-secret-change-me` |
| `CMS_ENV` | 设为 `production` 时**强制要求** `JWT_SECRET`（否则启动 panic） | 未设置 |
| `LLM_API_KEY` | 配置后 AI 草稿走真实大模型（默认 DeepSeek） | 未设置 → 模板兜底 |
| `LLM_BASE_URL` | 大模型 base（兼容 OpenAI） | `https://api.deepseek.com/v1` |
| `LLM_MODEL` | 模型名 | `deepseek-chat` |
| `PUBLIC_BASE_URL` | 回调/推送回跳前端地址 | `http://localhost:5188` |

### Turso 示例

```bash
export TURSO_URL="libsql://<db>.turso.io"
export TURSO_AUTH_TOKEN="<token>"
cargo run
```

> `CmsDb` 抽象层自动选择后端：Turso 走 HTTP `/v2/pipeline` 协议，业务代码无感知。

---

## 6. 生产注意事项

- **必须设置 `CMS_ENV=production` + `JWT_SECRET`**，否则后端拒绝启动（A2 安全治理）。
- 登录限速、Argon2 哈希、强制改密已内置；Secret 类字段（API Key / OAuth token）读出不回显。
- 当前为**单租户**（固定 `t_demo`）；多租户需后续 Phase 4 数据权限。
- 根目录 `data.db`（~36MB）为本地 SQLite 库，建议纳入 `.gitignore` 或明确用途。

---

## 7. 目录速记

```
admin/
├── server/        Rust/Axum 后端（单二进制 cms-server）
├── web/           React 前端（Vite）
├── docs/          本文档（准确版）；archive/ 为旧愿景文档
├── dist-app/      ← 与本项目无关的遗留 macOS 应用（OntologyOS.app），建议清理
└── data.db        本地 SQLite 数据文件
```
