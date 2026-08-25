# 架构说明

本系统遵循 **「AI 是执行者，不是超级管理员」** 的核心纲领（PRODUCT_VISION G4）：
所有写入动作都经过 `Permission → Capability → Policy → Approval → Action → Audit` 链路。

```
┌──────────────────────────────────────────────────────────────┐
│  Browser（React 19 + HeroUI + TanStack）                       │
│  Routes / Stores / Components  ── 权限镜像（config/permissions）│
└───────────────┬──────────────────────────────────────────────┘
                │  fetch /api/*  (Bearer JWT, 统一信封 {ok,data,total})
                ▼
┌──────────────────────────────────────────────────────────────┐
│  Axum Router（server/src/main.rs → build_router）              │
│                                                                │
│  Auth extractor  ── ensure(perm) 逐请求校验（权威）            │
│      │                                                         │
│      ├─ resources  统一资源网关（白名单表 + 列映射 + 权限绑定） │
│      ├─ ai          Capability→Policy→Action→Audit            │
│      ├─ approvals   裁决（领域动作，非泛型 CRUD）              │
│      ├─ automation  事件触发 + 工作流执行器                     │
│      ├─ scheduler   后台定时（分钟/时/日/周）                   │
│      ├─ oauth       OAuth2 授权码流                            │
│      ├─ team        成员管理                                   │
│      ├─ public_forms 匿名表单收集                              │
│      └─ notify      企微群机器人 Webhook                       │
└───────────────┬──────────────────────────────────────────────┘
                │
                ▼
┌──────────────────────────────────────────────────────────────┐
│  CmsDb 抽象层（server/src/cmsdb.rs）                           │
│   Local(SeaORM/SQLite)   ──   Turso(HTTP /v2/pipeline)        │
│   业务代码只依赖 CmsDb/Row，后端可切换                         │
└──────────────────────────────────────────────────────────────┘
```

---

## 后端分层（`server/src/`）

| 模块 | 职责 |
|------|------|
| `main.rs` | 组装路由（`build_router`）、启动引导、启动调度器 |
| `state.rs` | `AppState { db, tenant }`（Phase 1 单租户） |
| `db.rs` | 连接（TURSO_URL / 本地）、幂等建表、种子数据、旧库迁移 |
| `cmsdb.rs` | **数据库抽象**：Local vs Turso；`Row` / `FromDbVal` 行列映射 |
| `error.rs` | 统一响应信封：`{ok,data,total}` / `{ok:false,error}` + HTTP 状态码 |
| `auth.rs` | JWT 签发/校验、`Argon2` 哈希、登录限速锁定（A4）、`Auth` 提取器、`ensure()` 权限断言 |
| `perm.rs` | 角色矩阵（owner/editor/viewer）、`perm_matches`（精确 + `*` / `.*` 通配） |
| `resources.rs` | **统一资源网关**：白名单 `TableDef` + `ColDef`（snake↔camel、JSON 字段、Secret 掩码）、按权限 CRUD |
| `ai.rs` | AI 执行器：能力注册表、Policy（有权直执行 / 无码可升级则转审批 / 无码不可升级则 403）、审计、可选 LLM 生成 |
| `automation.rs` | `POST /api/automation/trigger` 事件入口 + 工作流执行核（notify/task/log 步骤）+ `/api/plugins` 能力清单 |
| `scheduler.rs` | 后台循环；`schedule.minutely/hourly/daily/weekly` 窗口去重触发 |
| `oauth.rs` | OAuth2 授权码流：google / github / mock provider |
| `team.rs` | 成员列表/邀请/角色变更（防自锁死） |
| `public_forms.rs` | 匿名表单读取与提交（落库 + 线索去重） |
| `notify.rs` | 企微群机器人 Webhook 推送（审批产生/完成通知） |
| `tests_api.rs` | 集成测试：登录/RBAC/限速/审批/公开表单关键链路 |

---

## 权限模型（单一事实源）

- **权限码**：点分格式 `域.资源.动作`（如 `content.articles.publish`），前后端共用同一套字面量。
- **后端权威**：`auth::ensure(perm)` 在 `Auth` 提取器中逐请求校验。
- **前端镜像**：`web/src/config/permissions.ts` 的 `ROLE_PERMS` 仅用于按钮/菜单显隐（UX），不替代后端校验。
- **角色**：
  - `owner`：`*`（全量），审批裁决者。
  - `editor`：可增删改内容/客户/线索，**但无 `*.publish`** —— 发布必须经 Owner 审批。
  - `viewer`：只读。
- **通配语义**：`*` 或 `content.articles.*` 命中尾段；角色矩阵一律显式枚举写权限，避免误授发布权。

---

## AI 审批闭环（G4 验收点）

```
用户(Editor) → "发布文章" → POST /api/ai/invoke {capability: content.articles.publish}
        │
        ├─ 有 content.articles.publish 权限？ → 直接执行，写审计
        └─ 无（Editor）？ → 生成 pending 审批（needs_approval）→ 通知 Owner
                                        │
Owner → POST /api/approvals/{id}/decide {status: approved}
        ├─ 幂等保护（已裁决不可重复）
        ├─ 批准后按 payload 执行（如 article → published）
        ├─ 写 ai_audit_log，并推送结果给发起人
```

---

## 双模式前端数据流

`web/src/api/cms/index.ts` 根据 `VITE_CMS_MODE` 选择适配器，页面代码零改动：

- **`mock`（默认）**：`collection<T>()` 走 `localStorage` + 模拟延迟，开箱即可演示。
- **`real`**：`httpCollection<T>('table')` → `GET/POST/PUT/DELETE /api/{table}`，与后端统一网关对齐。

所有请求经 `api/client.ts` 的 `request()` 统一处理：注入 Bearer、401→登出跳转、403→权限 toast、`{ok:false}`→抛 `ApiError`。

---

## 数据模型（`server/src/db.rs` 建表）

`tenants · users · articles · pages · products · media_items · customers · leads ·
forms · form_submissions · approvals · ai_tasks · workflows · ai_audit_log · integrations`

- 多租户字段 `tenant_id` 已就位（Phase 1 固定 `t_demo`），为 Phase 4 数据权限预留。
- 数组/对象字段以 JSON 字符串存 TEXT，读出时还原；Secret 类列（API Key / OAuth token）读出不回显明文。
- 启动时对旧库做幂等 `ALTER TABLE` 迁移（兼容历史列）。
