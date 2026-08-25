# 后端模块与 API 清单

> 基础路径：`http://localhost:8088`
> 统一请求头：`Authorization: Bearer <token>`、`Content-Type: application/json`
> 统一响应：成功 `{ "ok": true, "data": ..., "total"?: n }`；失败 `{ "ok": false, "error": "..." }`（HTTP 4xx/429）

---

## 模块与职责

| 文件 | 路由/能力 | 完成度 |
|------|-----------|--------|
| `auth.rs` | 登录、当前用户、改密、JWT、限速 | ✅ |
| `perm.rs` | 角色矩阵、通配匹配 | ✅ |
| `resources.rs` | 统一资源网关（11 张表 CRUD） | ✅ |
| `ai.rs` | AI 能力执行器 + 审计 | ✅ |
| `approvals`（resources.rs `decide`） | 审批裁决 | ✅ |
| `automation.rs` | 事件触发 + 工作流执行 + 插件清单 | ✅ |
| `scheduler.rs` | 后台定时调度 | ✅ |
| `oauth.rs` | OAuth2 授权码流 | ✅ |
| `team.rs` | 成员管理 | ✅ |
| `public_forms.rs` | 匿名表单收集 | ✅ |
| `notify.rs` | 企微 Webhook 推送 | ✅ |
| `cmsdb.rs` | SQLite / Turso 抽象 | ✅ |

---

## 接口清单

### 系统
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/healthz` | 健康检查（返回 phase/status） |

### 认证
| 方法 | 路径 | 权限 | 说明 |
|------|------|------|------|
| POST | `/api/auth/login` | 公开 | 登录，返回 `token` + `user` |
| GET | `/api/user/me` | 登录 | 当前用户 + 实时权限集 `permissions` |
| POST | `/api/me/password` | 登录 | 修改自身密码（≥8 位、不能与旧密码相同） |

### 统一资源网关（白名单表）
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/{table}` | 列表（支持 `?col=value` 白名单列等值过滤，按 `updated_at` 倒序） |
| POST | `/api/{table}` | 创建 |
| GET | `/api/{table}/{id}` | 读取单条 |
| PUT | `/api/{table}/{id}` | 更新 |
| DELETE | `/api/{table}/{id}` | 删除 |

可用 `{table}` 与权限前缀：

| table key | 表 | 读权限前缀 |
|-----------|----|-----------|
| `articles` | articles | `content.articles` |
| `pages` | pages | `content.pages` |
| `products` | products | `content.products` |
| `media` | media_items | `content.media`（上传/删除为独立码） |
| `customers` | customers | `business.customers` |
| `leads` | leads | `business.leads` |
| `forms` | forms | `business.forms` |
| `approvals` | approvals | `ai.approvals`（写收敛到裁决权） |
| `ai-tasks` | ai_tasks | `ai.tasks` |
| `workflows` | workflows | `automation.workflows`（启停为独立码） |
| `integrations` | integrations | `automation.integrations`（OAuth 凭证为 Secret 掩码） |

> 写动作的权限：默认 `.create/.update/.delete`；特殊语义动作由 `TableDef` 的 `create_perm/update_perm/delete_perm` 覆盖（如 `media.upload`、`ai.approvals.decide`、`automation.workflows.toggle`、`team.roles.manage`）。

### 审批
| 方法 | 路径 | 权限 | 说明 |
|------|------|------|------|
| POST | `/api/approvals/{id}/decide` | `ai.approvals.decide` | 裁决 `approved`/`rejected`；幂等；批准后执行 payload 携带动作（如发布文章）；写审计 + 推送结果 |

### 团队
| 方法 | 路径 | 权限 | 说明 |
|------|------|------|------|
| GET | `/api/team/users` | `team.users.view` | 成员列表 |
| POST | `/api/team/users` | `team.users.invite` | 邀请成员（初始密码 `demo1234`，强制改密） |
| PUT | `/api/team/users/{id}` | `team.users.invite` | 变更角色/昵称/启停（防自锁死） |

### AI
| 方法 | 路径 | 权限 | 说明 |
|------|------|------|------|
| POST | `/api/ai/invoke` | 登录 | 调用能力：`content.articles.draft/publish`、`content.seo.optimize`、`content.translate`；无码可升级则转审批（返回 `needs_approval`） |
| GET | `/api/ai/audit` | `ai.tasks.view` | 审计流水（支持 `?decision=`/`?capability=` 过滤） |

### 自动化
| 方法 | 路径 | 权限 | 说明 |
|------|------|------|------|
| POST | `/api/automation/trigger` | `automation.workflows.toggle` | 事件入口，匹配 `enabled` 且 `event` 相符的工作流并执行 |
| GET | `/api/plugins` | 登录 | 对外能力清单（与 AI 同源链路） |

### 公开收集（匿名）
| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/public/forms/{id}` | 读取已发布表单标题/描述（用于公开页） |
| POST | `/api/public/forms/{id}/submit` | 提交：`{name, phone?, interest?, note?}`；落 `form_submissions`、计数 +1、有手机号则建线索（同表单同号去重） |

### OAuth2
| 方法 | 路径 | 权限 | 说明 |
|------|------|------|------|
| GET | `/api/integrations/{id}/oauth/start` | `automation.integrations.toggle` | 返回三方授权 URL + state |
| GET | `/api/integrations/{id}/oauth/status` | `automation.integrations.view` | 是否已授权 |
| GET | `/api/integrations/oauth/callback` | 公开（匿名） | 三方回调：校验 state → code 换 token → 存库 → 302 回前端 |

---

## 测试覆盖

- **单元测试**：`perm`（通配/角色矩阵）、`cmsdb`（行解析/值解析）、`scheduler`（窗口计算 + 同窗口不重复触发）。
- **集成测试**（`tests_api.rs`，对 `build_router` 直接发请求，无需起端口）：
  - 登录成功 + `/me` 返回权限
  - 密码错误 5 次 → 第 6 次 429 锁定
  - Editor 不能邀请成员、Owner 可以、受邀者强制改密
  - 发布审批闭环：Editor 触发转审批 → Owner 批准 → 文章变 `published`
  - 公开表单提交建线索 + 同号去重

> 运行：`cd server && cargo test`
