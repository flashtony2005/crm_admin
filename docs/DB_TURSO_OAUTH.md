# 数据库：本地 SQLite 或 Turso（libsql）

## 双后端架构

`src/cmsdb.rs` 提供统一数据库抽象 `CmsDb`，业务代码只依赖 `CmsDb` / `Row`：

| 模式 | 触发条件 | 连接方式 |
|---|---|---|
| 本地 SQLite | 未设置 `TURSO_URL` | `sqlite://cms.db`（开发/测试默认） |
| Turso 远程 | 设置 `TURSO_URL` + `TURSO_AUTH_TOKEN` | libsql HTTP 协议（`/v2/pipeline`） |

选择依据：sea-orm 1.1 无 libsql driver（2.x 需网络升级），Turso 官方
HTTP 协议为纯 JSON，用已有 reqwest 实现，零新增运行时依赖。
`Row` 双形态：本地 = 原生 QueryResult（按列名读），Turso = 显式列名映射。

## 启用 Turso

```bash
# 1) turso CLI 建库拿 URL 与 token（https://docs.turso.tech）
turso db create cms-demo
turso db show cms-demo --url      # → libsql://xxx.turso.io
turso db tokens create cms-demo   # → 一串 token

# 2) 启动服务（表结构与种子会在首次启动自动建）
TURSO_URL=libsql://xxx.turso.io \
TURSO_AUTH_TOKEN=<token> \
PUBLIC_BASE_URL=https://your-domain \
PORT=8088 ./target/debug/cms-server
```

- 表结构：`src/db.rs` 编译期固定 DDL（TD-1），启动幂等建表 + 空库种子；
- 旧库字段变更以幂等 `ALTER TABLE` 迁移；
- `PUBLIC_BASE_URL` 决定 OAuth 回调地址与推送链接前缀。

## 本地验证协议（mock libsql server）

开发时可起一个本地 libsql 协议 mock（`python3` 实现 `/v2/pipeline`，
落盘 sqlite 文件），用 `TURSO_URL=http://127.0.0.1:9099` 指向它跑全链路冒烟
（登录 / RBAC / 审批 / 表单 / 工作流 / 审计 10 项全过）。

## OAuth2 集成（B6）

Integrations 页「连接」分两种：

- **OAuth2 授权码**（ga/gsc → google，github）：填 client_id/secret →
  服务端生成授权 URL（state 10 分钟有效）→ 三方授权 → 回调
  `/api/integrations/oauth/callback` 换 token 存库（Secret 掩码，不回显）→
  302 回前端标记页；
- **API Key**（wecom-bot 等）：粘贴 Key 即连。

provider 注册表在 `src/oauth.rs`（google / github / mock）。
回调地址：`{PUBLIC_BASE_URL}/api/integrations/oauth/callback`
（需在 Google Cloud Console / GitHub OAuth Apps 中登记）。
