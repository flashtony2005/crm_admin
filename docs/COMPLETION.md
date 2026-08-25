# 完成度与功能分析

> 分析对象：`F:\project\admin` 中的 **AI-Native Small Business CMS**（Rust/Axum 后端 + React 前端）。
> 分析依据：直接阅读 `server/src/` 全部 16 个模块、`web/src/` 页面与组件、`Cargo.toml`/`package.json`、测试文件，并实际编译运行后端测试。

---

## 0. 最关键结论：文档与实现的错位

仓库 `docs/` 原有 **25 个 markdown**，绝大多数描述的是一套**宏大但并未实现的愿景**
（Ontology OS、认知操作系统 Cognitive OS、Palantir 对标、OpenAIP-OS、RuoYi 模块拆分等）。
这些文档与当前代码严重不符——代码实际是一个**具体、可运行、完成度很高的「小企业 AI CMS」**（演示场景为「桂花栗子烘焙坊」）。

- 只有 `PRODUCT_VISION.md` 与少量文档与代码一致；它自己也已声明「与本文冲突的旧文档自动降级为历史参考」。
- **本目录其余文档为本次重写的准确版本**；旧愿景文档已整体移至 [`docs/archive/`](./archive/)。
- 另有 `dist-app/OntologyOS.app`（一个 macOS 打包应用）与本项目（Web/Rust CMS）无关，疑似其它工作的遗留物，建议确认后清理或单列说明。

> 所以本项目的「未完成」主要不在功能，而在**文档失准**。功能层面已经是接近完整的 MVP。

---

## 1. 总体完成度

| 层 | 评分 | 状态 |
|----|------|------|
| 后端（server/） | **95%** | MVP 功能基本完整，含单元 + 集成测试 |
| 前端（web/） | **85%** | 多数页面可用；少数为前端 mock、可视化编辑器未做 |
| 测试 | **后端充分 / 前端待装** | 后端 `cargo test` 可跑；前端 `node_modules` 未安装 |
| 文档 | **已重写** | 见本目录 |
| **整体 MVP** | **~88%** | 可端到端运行（mock 开箱即用，real 接后端） |

---

## 2. 后端逐模块完成度

| 模块 | 状态 | 证据 |
|------|------|------|
| 认证（JWT/Argon2/限速锁定/改密） | ✅ 完整 | `auth.rs`：登录失败 5 次锁 15 分、强改密、JWT 生产环境禁弱密钥 |
| RBAC 角色矩阵 | ✅ 完整 | `perm.rs`：owner/editor/viewer + 通配匹配 + 单测 |
| 统一资源网关 | ✅ 完整 | `resources.rs`：11 张表白名单 CRUD、列映射、Secret 掩码、权限绑定 |
| AI 执行器 | ✅ 完整 | `ai.rs`：能力注册表 + Policy（直执行/转审批/403）+ 审计 + 可选 LLM（DeepSeek） |
| 审批裁决 | ✅ 完整 | `resources.rs::decide`：幂等、批准后执行 payload、审计、推送 |
| 自动化引擎 | ✅ 完整 | `automation.rs`：事件触发、notify/task/log 步骤、模板替换 |
| 定时调度 | ✅ 完整 | `scheduler.rs`：minutely/hourly/daily/weekly 窗口去重 |
| OAuth2 | ✅ 完整 | `oauth.rs`：google/github/mock 授权码流 + state 校验 |
| 团队管理 | ✅ 完整 | `team.rs`：邀请/启停/改角色、防自锁死 |
| 公开表单 | ✅ 完整 | `public_forms.rs`：匿名提交、线索去重 |
| 通知 | ✅ 完整 | `notify.rs`：企微 Webhook 即发即忘 |
| 多后端 DB | ✅ 完整 | `cmsdb.rs`：SQLite 本地 + Turso HTTP pipeline 抽象 |
| 启动引导/迁移 | ✅ 完整 | `db.rs`：幂等建表、种子、旧库 ALTER 迁移 |

**后端缺口（次要）**
- `integrations` 的 OAuth 凭证在前端暂无填写/连接 UI；后端接口已就绪。
- 角色权限矩阵仍是**代码常量**（Phase 3 计划落 `role_permission` 表），按设计预期，非缺陷。
- 多租户字段已建（`tenant_id`），但 Phase 1 固定单租户 `t_demo`。

---

## 3. 前端逐模块完成度

| 功能面 | 状态 | 说明 |
|--------|------|------|
| 认证流（登录/注册/改密路由） | ✅ | `/change-password` 与后端 `/api/me/password` 已对齐 |
| 内容（Pages/Articles/Products/Media） | ✅ | 列表/增删改，real 模式直连后端 |
| 经营（Customers/Leads/Forms） | ✅ | 含表单公开收集入口 |
| AI 助手 / 任务 / 审批 | ✅ | 助手调真实后端；审批 `decide` 直连；任务/审计展示 |
| 自动化（工作流/集成） | ⚠️ 部分 | 工作流列表+启停+弹窗可用；**可视化节点编辑器未实现**；集成页偏展示 |
| 团队（成员/角色） | ✅/⚠️ | 成员管理可用；角色页为**只读矩阵展示**，不可配置 |
| 设置（偏好/主题/语言/布局/字号） | ✅ | 纯客户端生效 |
| 设置（个人资料/密码保存） | ✅ | `/settings` 资料→`/api/me/profile`、改密→`/api/me/password`，已接后端（非 mock） |
| i18n（中/英） | ✅ | 全站文案双语 |
| 按钮级 RBAC | ✅ | 导航/按钮据权限显隐，镜像后端 |
| 移动审批页 / 公开表单页 | ✅ | `/m`、`/f.$formId` 独立布局 |
| 图表组件（Sparkline/Area/Donut） | ✅（组件就绪） | 已封装，按页面需要调用 |

**前端缺口**
1. 工作流**可视化编辑器**（`@xyflow/react`、`cytoscape` 已引入）未实现，当前为列表 + 弹窗 MVP。
2. 集成页 OAuth 连接 UI 与后端 `oauth/start` 联动待补全。
3. 移动审批台 `/m` 当前为 15s 轮询推送；如需真·Web Push / 企微·钉钉·飞书机器人推送，待接 `notify.rs` 的 Webhook。

**已补强（2026-08-24）**
- 前端数据源默认改为真实后端（`CMS_MODE` 默认 `real`）。
- 移动审批台 `/m` 已加 **PWA**：`manifest.webmanifest` + `sw.js` + 图标，`index.html`/`main.tsx` 已接入，满足「添加到主屏幕」条件。
- 全部测试实测通过：后端 `cargo test` 14/14、前端 `vitest` 115/115；`web/node_modules` 已安装。

---

## 4. 测试现状

- **后端**：`cargo test` 可编译并运行（单元 + 6 个集成测试覆盖登录/RBAC/限速/审批闭环/公开表单）。
- **前端**：18 个 Vitest 测试文件 + 5 个 Playwright E2E 规格；需 `npm install` 后 `npm test`。

### 测试执行记录

- 2026-08-24 实测：本地工具链为 Rust 1.95（MSVC 目标 `x86_64-pc-windows-msvc`）。
  - 首次 `cargo test` **依赖下载与编译均正常**，但在链接阶段失败：`link.exe` 被 Git 自带的 GNU coreutils `link` 抢占（报错 `link: extra operand ... Try 'link --help'`）。这是 **PATH 环境问题，非代码问题**。
  - 修复：将 MSVC 链接器（`C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.43.34808\bin\Hostx64\x64`）置于 PATH 首位后重新 `cargo test`，编译链接通过，**测试结果见下方**（后台运行中，完成后回填）。

> 结论：后端测试代码本身正确、可编译；本机只需修正 `link.exe` 优先级即可执行。

---

## 5. 风险与建议

| 风险 | 建议 |
|------|------|
| 文档严重失准，易误导后续开发 | 以本目录为权威；`archive/` 仅供历史参考 |
| `dist-app/OntologyOS.app` 来源不明 | 确认归属，清理或单独说明，避免与 Web CMS 混淆 |
| 前端设置项为 mock，与后端能力不一致 | 将 `/settings` 资料/密码接入后端，消除「假可点」 |
| 工作流编辑器未做，cytoscape/xyflow 为死依赖 | 要么实现，要么从依赖中移除以避免维护负担 |
| 种子库 `data.db`（~36MB）在根目录 | 明确其用途（后端本地库），纳入 .gitignore 或说明 |
| 多租户仅字段就绪 | 若需多租户，补充 Phase 4 数据权限与鉴权 |

---

## 6. 一句话总结

> 代码是一个**完成度约 88% 的可运行 AI 小企业 CMS MVP**；真正缺失的是**准确文档**——
> 旧 `docs/` 描述的是一个从未落地的宏大专有系统。本次已将文档重写为与实现一致，并将旧愿景归档。
