# AI-Native Small Business CMS

> 一个面向小企业的 **AI 数字工作台**：用自然语言让 AI 帮忙管网站、内容、客户与日常运营。  
> 后端：Rust + Axum；前端：React 19 + HeroUI + TanStack。单租户 MVP（Phase 1/2）。

---

## 文档导航

| 文档                                   | 内容                        |
| ------------------------------------ | ------------------------- |
| [COMPLETION.md](./COMPLETION.md)     | **完成度与功能分析**（逐模块评分、缺口、风险） |
| [ARCHITECTURE.md](./ARCHITECTURE.md) | 整体架构、分层、权限链路、双模式数据流       |
| [BACKEND\_API.md](./BACKEND_API.md)  | 后端模块说明 + 全部 HTTP 接口清单     |
| [FRONTEND.md](./FRONTEND.md)         | 前端页面、组件、状态、路由、i18n        |
| [DEPLOYMENT.md](./DEPLOYMENT.md)     | 本地运行、构建、环境变量、前后端联调        |



> 历史设计文档（愿景/本体/认知 OS 等，多数与当前实现不符）已归档至 [`docs/archive/`](./archive/)。

---

## 技术栈

**后端（ `server/`，单二进制 `cms-server` ）**

- Rust 2021 · Axum 0.8 · Tokio · SeaORM 1.1（`sqlx-sqlite`）· `sqlx` 0.8
- 认证：`jsonwebtoken` + `argon2`（bcrypt 风格哈希）
- 外部调用：`reqwest`（LLM 兼容接口、OAuth、企微 Webhook）
- 数据库：**本地 SQLite**（SeaORM）或 **Turso 远程**（HTTP `/v2/pipeline` 协议），通过环境变量切换

**前端（ `web/`，Vite 构建）**

- React 19 · TypeScript · Vite 8 · Tailwind 4
- TanStack Router / Query / Table / Form
- HeroUI 3（UI 组件）· Zustand 5（状态）· i18next（中/英）
- 图表：`chart.js`；图编辑：`cytoscape` + `@xyflow/react`（预留）
- 测试：Vitest 3（单元/组件） + Playwright 1.61（E2E）

---

## 功能范围（已实现）

- **内容**：Pages / Articles / Products / Media 的增删改查
- **经营**：Customers（客户）、Leads（线索）、Forms（表单与公开收集）
- **AI 助手**：自然语言触发「写草稿 / 发布」等能力；发布等高风险动作自动转 **Owner 人工审批**
- **审批中心**：待办裁决（批准/驳回），全程审计留痕
- **自动化**：事件触发的工作流（欢迎消息、经营摘要等）+ 定时调度（分钟/时/日/周）
- **团队**：成员邀请/启停、角色权限矩阵可视化
- **集成**：OAuth2 接入（Google / GitHub / 本地 mock）+ 企微群机器人推送
- **安全**：JWT、Argon2 密码哈希、登录失败限速锁定、强制改密、按钮级 RBAC

---

## 完成度速览（详见 COMPLETION.md）

| 层          | 完成度      | 说明                                |
| ---------- | -------- | --------------------------------- |
| 后端         | **~95%** | MVP 功能基本完整，含单元 + 集成测试             |
| 前端         | **~85%** | 多数页面可用；少数设置项为前端 mock、可视化工作流编辑器未实现 |
| 文档         | **已重写**  | 旧愿景文档已归档，本目录为准确文档                 |
| **整体 MVP** | **~88%** | 可端到端运行（mock 模式开箱即用，real 模式接后端）    |

---

## 一分钟上手

```bash
# 1) 启动后端（默认 SQLite，端口 8088）
cd server && cargo run
#    默认账号：owner / editor / viewer，密码均为 demo1234

# 2) 启动前端（默认 mock 模式，端口 5188，数据存 localStorage）
cd web && npm install && npm run dev

# 3) 想接真实后端：前端以 real 模式启动
cd web && VITE_CMS_MODE=real npm run dev
#    vite 已配置 /api 代理到 http://localhost:8088
```

> 默认 `CMS_MODE=mock`：前端用 localStorage 适配器即可完整演示，无需后端。  
> 设置 `VITE_CMS_MODE=real` 后，页面零改动切换到 Axum 后端（接口契约一致）。
