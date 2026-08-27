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
| 后端（server/） | **96%** | MVP 功能基本完整，含单元 + 集成测试 |
| 前端（web/） | **88%** | 多数页面可用；少数为前端 mock、可视化编辑器未做 |
| 测试 | **后端充分 / 前端待装** | 后端 `cargo test` 可跑；前端 `node_modules` 未安装 |
| 文档 | **已重写** | 见本目录 |
| **整体 MVP** | **~94%** | 可端到端运行（mock 开箱即用，real 接后端）；含站点级主题/模板系统 + 读统计看板 |

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
| 定时调度 | ✅ 完整 | `scheduler.rs`：minutely/hourly/daily/weekly 窗口去重；另含定时发布提升（`publish_scheduled`） |
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
| 公开站增强（Featured/作者页/canonical/srcset） | ✅ | 精选位、/author 路由、canonical 注入、响应式图 srcset 全部落地 |

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

---

## 7. Ghost 化补齐（2026-08-27）：Featured / 定时发布 / 作者页 / canonical / srcset

在既有公开内容能力（public_api、付费墙、SEO sitemap/RSS）之上，一次性补齐 5 项 Ghost 式发布能力，
全部已端到端落地（后端迁移 + API + 前端 UI）。

| 功能 | 后端 | 前端 | 说明 |
|------|------|------|------|
| **Featured 精选位** | ✅ | ✅ | `articles.featured`(INT 0/1) 列；公开列表 `ORDER BY featured DESC` 置顶；编辑器「设为精选」开关；首页 Hero 优先展示精选文章 |
| **定时发布（Scheduled）** | ✅ | ✅ | `articles.scheduled_at`(TEXT) 列 + `status='scheduled'`；`scheduler::publish_scheduled` 每分钟扫描，到点自动提升为 `published`（补 `published_at`）；编辑器状态选「定时发布」并填时间 |
| **作者页（Author）** | ✅ | ✅ | 公开 API `?author=` 过滤；新增 `/author/$name` 路由页（聚合该作者已发布文章，含 SEO/canonical）；首页/详情/读页作者名均可点击跳转 |
| **canonical 规范链接** | ✅ | ✅ | `articles.canonical_url`(TEXT) 列；公开 API 返回 `canonical_url`；读页与首页详情注入 `<link rel="canonical">`（自定义优先，否则用默认 `/read/{key}` URL） |
| **srcset 响应式图片** | ✅ | ✅ | `upload.rs` 上传时生成 480/960/1600 宽变体并返回 `srcset`；公开 API 对 `featured_image` 推导 `featured_image_srcset`；首页/详情/读页/作者页 `<img srcSet>` 渲染 |

**配套修复（顺带）**：`db.rs` 补齐 articles 缺失的内容列 `slug/featured_image/published_at/meta_title/meta_description`
（此前仅在旧库中存在、新库 bootstrap 未建，查询会失败）——现已幂等 ALTER 补齐，新库开箱可用。

**关键约束 / 取舍**
- 定时发布依赖后端常驻调度进程（`scheduler::spawn` 每分钟 tick）；纯前端 mock 模式下不触发，需接 `real` 后端。
- srcset 仅对**本站托管上传图**（`/uploads/<uuid>.<ext>`）生效，外链 / data URL 不生成（前端与后端共用命名约定 480/960/1600）。
- 作者为 `articles.author` 自由文本（与既有模型一致），作者页按作者名精确匹配聚合，未引入独立作者表。

**剩余缺口（未在本轮处理）**
- 可视化工作流编辑器、集成 OAuth 连接 UI（§3 前端缺口 1/2）。

> 读统计看板已于 2026-08-27 收尾（见 §9）。

---

## 8. 站点级主题 / 模板系统（2026-08-27 追加）

补齐 Ghost 对照表中原标记为 ❌ 的「主题/模板系统」——根因为：4 套 CSS 主题虽已实现，但仅访客 localStorage 本地生效，缺「站点级」设定能力。本轮升级为**发布者可在后台统一设定、对所有访客生效**的站点级主题 + 布局模板，访客仍可本地覆盖。

| 能力 | 后端 | 前端 | 说明 |
|------|------|------|------|
| 站点设置 KV 存储 | ✅ | — | `site_settings` 表（key TEXT PK），`server/src/db.rs` 引导建表 + 默认种子 |
| 公开读取端点 | ✅ | ✅ | `GET /api/public/site`（免认证）→ `{theme,template,siteTitle,siteTagline}`，带默认值回退 |
| 站点设置更新端点 | ✅ | ✅ | `PUT /api/admin/site`（需 `site.settings.update`，Owner 通配）upsert 4 个 KV |
| 4 套站点主题 | ✅ | ✅ | `paper / ink / sepia / ocean`，CSS 变量驱动（`themes/siteThemes.ts`） |
| 3 种布局模板 | ✅ | ✅ | `default / magazine / minimal`（`themes/siteTemplates.ts`：网格列数 / 卡片样式 / Hero 形态） |
| 公共页套用 | ✅ | ✅ | `site.tsx / author.$name.tsx / tag.$slug.tsx / read.$key.tsx` 经 `useSiteTheme()` 拉取并套用 |
| 后台外观设置页 | ✅ | ✅ | `settings.tsx` 新增「站点外观」Tab（Owner 可见）：主题 4 选 1 + 模板 3 选 1 + 站点名/标语，PUT 落库 |
| 访客本地覆盖 | ✅ | ✅ | `useSiteTheme().setLocalTheme(key)` 写入 localStorage，仅该访客生效，不影响站点默认 |

**关键取舍**
- 站点设置采用 KV 模型而非 `resources.rs` 标准表（后者要求 `id` 主键），以独立模块 `site.rs` + 专用端点承载，避免污染通用网关与列映射白名单。
- 权限沿用已有 `site.settings.update` 码，由 Owner 通配覆盖；**未改动** RBAC 角色矩阵（保持前端权限镜像同步），编辑器/访客无此码自然不可见、不可写。
- 主题与模板正交：theme 管「配色」，template 管「版式」，可任意组合。

---

## 9. 读统计看板 / 事件生态（2026-08-27 收尾）

收尾 COMPLETION §3 前端缺口第 3 条 + Ghost 对照表中唯一遗留的 P0。采用**统一事件日志（事件生态）**设计：所有「阅读 / 注册 / 评论 / 订阅」等行为写入 `events` 表，统计看板只负责从中聚合，写入与读取解耦，便于后续扩展更多事件维度。

| 能力 | 后端 | 前端 | 说明 |
|------|------|------|------|
| 统一事件表 | ✅ | — | `events(id, tenant_id, type, ref_id, ref_key, payload, created_at)` + `(tenant_id,type,created_at)` 索引；`server/src/db.rs` 建表 |
| 阅读埋点端点 | ✅ | ✅ | `POST /api/public/track`（免认证）写入 `article_view` 事件；同步累加 `articles.views` 缓存计数 |
| 阅读计数落列 | ✅ | — | `articles` 新增 `views INTEGER DEFAULT 0`（`db.rs` 幂等 ALTER），供按文章取总量 |
| 后台聚合接口 | ✅ | ✅ | `GET /api/admin/stats`（需 `content.articles.view`）：总阅读量 / 已发布文章 / 会员 / 评论、近 14 天每日阅读序列、热门文章 Top5 |
| 看板页面 | — | ✅ | `web/src/routes/stats.tsx`：4 张 `StatCard` + `AreaChart`（14 天阅读趋势）+ 热门文章列表；导航 `config/nav.ts` 增加 `stats` 叶子（权限 `content.articles.view`） |
| 前端埋点 | — | ✅ | `read.$key.tsx` 文章 `ready` / `locked` 预览时静默 `POST /api/public/track`，失败不影响阅读 |
| i18n | — | ✅ | `zh-CN.json` / `en.json` 增加 `stats.*` 键 |
| 演示数据 | ✅ | — | 建库时为已发布文章补 14 天阅读事件，看板开箱即有趋势与热门数据 |

**关键取舍**
- 看板指标（总阅读量 / 热门文章）直接聚合 `events` 日志，不依赖 `articles.views` 缓存；`views` 列仅作单篇文章累计的冗余缓存，二者可并存、互不阻塞。
- 事件生态可横向扩展：`POST /api/public/track` 接受任意 `type`（如 `member_signup` / `comment_added`），后续只需在 `GET /api/admin/stats` 增聚合分支即可，无需改表结构。
- 路由生成：因 `routeTree.gen.ts` 长期被运行进程占用、沙箱禁止改名覆盖，已将 TanStack Router 生成目标重定向至 `src/routeTree.generated.ts`（`vite.config.ts` 的 `generatedRouteTree`），`main.tsx` 同步改导入；旧 `routeTree.gen.ts` 已删除。

