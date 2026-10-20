# 项目总结与架构反省

**日期**：2026-10-19  
**范围**：`F:\project\admin`（后端 `server/` + 运营后台 `web/`）+ 主页仓库 `F:\project\coucouya`  
**方法**：所有数字来自源码静态统计与 `server/cms.db` 实测查询，不是文档转录；验证命令见文末附录。  
**定位**：私有化「创作者变现站」——CMS + 付费会员 + 社区 + 积分 + 后台，单实例、单站点、完全自托管。

---

## 0. 一页结论

**这个项目的工程质量，明显高于它的验证程度。**

- **工程侧**：单二进制 + SQLite、18 个版本化迁移、39 个后端模块、91 条路由、88 个后端测试。几个最容易出错的地方——订阅状态机、付费墙判据、Agent 事实源、CRUD 网关——都收敛到了单一出口，并用测试钉住了历史上的真实事故。这在单人项目里是少见的纪律。
- **产品侧**：**真实用户 0、真实收入 0、真实流量事件只有 `article_view` 一种**。58 张表里 21 张是空的；29 张网关表里 14 张零数据（48%）；已发布的 5 篇文章全部是免费公开内容，**付费墙从未被真实内容触发过**。
- **核心判断**：最大的架构问题不是「代码写得不好」，而是**功能面按「平台应该有什么」扩张，而不是按「谁在用什么」收敛**。这与此前 `0003` 那次「先建 12 张表、后找消费方、最终整层删除」是同一个模式——只是这次留在了库里。

**三句话建议：**

1. **先让 4 篇文章变成能收钱的商品**：补内容、至少 1 篇真正付费、用现有的 manual 通道走一笔真实订单。不要继续加功能。
2. **把所有「双份维护」的地方机械化**：权限码、前端数据层、README/迁移。这是本项目历史资损事故的共同根因。
3. **在真实转化出现之前冻结新对象模型**：Context/Intent、多租户、`blocks` 都先不做。

---

## 1. 现状快照（2026-10-19 实测）

### 1.1 代码与结构规模

| 维度 | 实测值 |
|---|---|
| 后端模块 | 39 个 `.rs`，共 19,873 行 |
| 后端路由 | 91 条 `.route()` 注册 |
| 数据库迁移 | 18 个版本（`0001`–`0018`，全部已应用） |
| CRUD 网关 | 29 张表（`TableDef`） |
| 数据库表 | 58 张（含迁移账本 `_migrations`），其中 21 张零行 |
| 前端页面 | 46 个路由 `.tsx`，TS/TSX 共 23,713 行 |
| 权限码 | 前端 79 个 / 后端 74 个（已漂移，见 R3） |
| 测试 | 后端 88 个 `#[test]`；前端 21 个测试文件 |
| 技术债标记 | `TODO/FIXME/unimplemented` = **0**；非测试代码 `unwrap/expect` = 49 |
| CI | **无** `.github/workflows` |
| 构建产物 | `web/dist` 停留在 2026-09-22，落后源码约 4 周 |

### 1.2 已建成的功能面

| 域 | 能力 |
|---|---|
| 内容 | 文章/页面/标签/分区/导航/置顶/模板版本、定时发布、SEO 重定向、404 监控、Smart Links |
| 会员 | 注册/登录/改密、会员档案、会员标签、邀请码、积分账本 |
| 收款 | 订单、套餐、订阅、微信支付 API v3、Stripe（手写 HTTP）、manual 人工开通、webhook 幂等 |
| 权益 | `sub_state` 订阅状态机、付费墙（`visibility` + `paid_level`）、402 结构化预览 |
| 社区 | 评论、举报、反应、审批流 |
| 自动化 | 工作流、集成、AI 任务与审批、AI 审计 |
| Agent | `/llms.txt`、`/agent.json`、`/ai/product`、JSON-LD、21 个 AI 爬虫白名单 |
| 分析 | 首触归因（visitor → 会员 → 收入）、统计看板、事件表 |
| 平台 | RBAC（前端 79 / 后端 74 权限码，见 R3）、团队、审计日志、多语言、GraphQL、OAuth、邮件、上传 |

### 1.3 数据真相（本节最重要）

| 对象 | 实测 | 判断 |
|---|---|---|
| 会员 | **19**，邮箱全部为 `@test.dev` | 无真实用户 |
| 订单 | 13：5 paid / 7 pending manual / 1 pending points | 5 笔已支付**全部是测试** |
| 订阅 | 1 行，`canceled`，ID 为 `sub_e2e_stage1` | 仅端到端测试 |
| 事件 | 61，**全部是 `article_view`** | 无注册/下单/订阅事件，漏斗不存在 |
| 已发布文章 | 5 篇：4 篇 12,302–18,466 字符，**1 篇仅 97 字符** | 供给极薄，且有 1 个空壳已上线 |
| 付费文章 | **0**：已发布文章全部 `visibility=public, paid_level=0, price_points=0` | 付费墙从未被真实内容触发 |
| 套餐 | 2 行（月度 ¥18 / 年度 ¥180），但 `stripe_price_id` 与 `stripe_price_yearly_id` **均为空** | Stripe 在线支付不可用，仅 manual 通道可用 |
| 问题页 | **0**（`kind` 全为 `post`） | `/problems` 体系无内容 |
| 404 日志 | 1 条，UA 为 `curl` | 无真实流量信号 |
| 空表 | 21/58（36%）；29 张网关表中 14 张零数据（48%） | 功能面远超使用面 |
| 遗留表 | 9 张零代码引用但仍有数据 | `0003` 的架构沉积层 |

> **与 2026-09-22 那次评估相比的进步**：`tiers` 从 0 行变成 2 行、`events` 从 0 变成 61、迁移 `0015` 已应用。三个阻塞项解决了两个——**剩下最硬的那个是内容供给**。

### 1.4 运行与交付

- 单二进制 `cms-server` + 本地 SQLite `cms.db`；可选 Turso/libsql 双后端，由 `CmsDb` 抽象。
- 后台：5188（Vite dev）/ 8088（API + 静态 SPA + `/t/*` 模板 + 机器端点）。
- 主页：独立仓库 `coucouya`（5199），构建产物部署到 `server/templates/coucouya/`。
- 调度器：进程内每分钟 tick（发布、webhook 重试、浏览统计落库、订阅到期）。
- 部署半径小是真实优势：除可选的 SMTP/支付/LLM 外，没有外部强依赖。

---

## 2. 架构现状

### 2.1 分层图

```
浏览器 / Agent / 搜索引擎
        │
        ├── HTML（CSR SPA + 服务端注入 OG / Twitter / JSON-LD）
        ├── 机器端点：/llms.txt  /agent.json  /ai/product  /sitemap.xml  /rss.xml
        └── 公开 API：/api/public/*
                    │
              Axum 路由（91 条）
                    │
   ┌────────────────┼────────────────┬─────────────────┐
   │ 内容 / 会员     │ 收款 / 权益     │ Agent / 分析     │
   │ articles       │ orders          │ agent_layer      │
   │ members        │ tiers           │ attribution      │
   │ comments       │ sub_state       │ stats            │
   │ points         │ webhooks        │ seo / seo_redirect│
   └────────────────┴────────┬───────┴─────────────────┘
                             │
                   resources.rs（29 表 CRUD 网关）
                             │
                          CmsDb
                    ┌────────┴────────┐
                 SQLite            Turso/libsql
```

### 2.2 单一事实源清单（本项目最值得保留的设计）

| 事实 | 唯一出口 | 价值 |
|---|---|---|
| 会员是否有效 | `sub_state.rs` | 判据只看 `current_period_end` vs `now`，与「收到哪个事件」解耦；事件、调度器、后台共用同一 `apply_row` |
| Agent 能看到什么 | `agent_layer.rs::Facts` | `/llms.txt`、`/agent.json`、`/ai/product` 同源生成，不会三份口径 |
| 付费墙 | `is_gated(visibility, paid_level)` | 公开 API 与 Agent Layer 共用同一判据 |
| 表 CRUD | `resources.rs::TableDef` | 29 张表一个网关，列演进集中管理 |
| 内容 URL | `seo::content_url()` | 页面、sitemap、RSS、JSON-LD 分流唯一出口 |

### 2.3 三条关键链路

1. **内容**：`articles` → `public_api` → 页面 / SEO / RSS / Agent Layer  
2. **收款**：`tiers` → `orders` →（manual / Stripe / 微信）→ `subscriptions` → `sub_state` → 权益  
3. **归因**：匿名 `visitor_id` → 注册回填 `members.first_touch_article_id` → 订单 → 收入

---

## 3. 架构做得对的地方

1. **用状态机而不是事件流表达「会员是否有效」**。这是全项目最正确的一个决定。大多数同类系统会在 webhook 里直接改 `plan`，然后被乱序/重放/漏事件打穿；这里把「事实」压到 `period_end` 一个可验证的值上，事件只负责落库。  
2. **把「对外声明」收敛到一份 Facts**。`/llms.txt`、`/agent.json`、`/ai/product` 不各自拼装，避免了「三个入口三种说法」的典型 Agent 反模式。  
3. **用测试钉死历史事故**。88 个后端测试覆盖了 DTO camelCase 契约、幂等 `rows_affected > 0`、付费墙判据、目录穿越、SPA 兜底 404 等真实踩过的坑——测试不只是覆盖率，是事故档案。  
4. **迁移内联、版本化、可单文件交付**。没有 sqlx-cli 依赖，`_migrations` 账本清晰，`0001`–`0018` 可追溯；对私有化部署是加分项。  
5. **双后端抽象（SQLite / Turso）藏在 `CmsDb` 后面**。切换云库只改连接串，业务代码不感知，部署弹性保留住了。

---

## 4. 架构反省

> 以下按「对当前阶段的伤害程度」排序，不按代码美观度排序。

### R1　功能面与需求面倒挂（最严重）

**现象**：系统有 58 张表、29 张表的 CRUD 网关、46 个后台页面、79 个权限码；但真实用户 0、真实订单 0、真实事件只有浏览。

**证据**：

- 21/58 张表零行；网关表已确认 14 张零数据：`pages`、`products`、`media_items`、`customers`、`leads`、`forms`、`ai_tasks`、`integrations`、`invite_codes`、`redeem_codes`、`points_ledger`、`subscribers`、`webhook_subscriptions`、`article_dispatches`。
- 46 个后台页面里，大量页面服务的是上述空表。
- 79 个权限码对应 19 个测试会员——权限矩阵的复杂度远超被保护资产的数量。

**根因**：功能以「一个 CMS 平台应该有什么」为驱动，而不是以「本周有谁在用、下一笔收入从哪来」为驱动。这是典型的 **speculative generality（投机性泛化）**。

**代价**：每多一张空表，就要多付一份迁移、网关列定义、权限码、前端页面、测试与文档的维护成本；更贵的是**认知成本**——新人（包括未来的你）无法从代码判断哪些是「在用」，哪些是「占位」。

**建议**：给每张空表做一次「保留 / 删除 / 标记实验」三选一，写进 `docs/` 一张清单。空表不是罪，**没有主人的空表**才是。

---

### R2　内容供给与商业化没有闭环

**现象**：变现链路建得很完整，但商品本身还没上架。

**证据**：

- 5 篇已发布文章中，`android-overseas-subscription-payments` **只有 97 个字符**，却处于 `published` 状态——这是比草稿更糟的状态：搜索引擎和 Agent 会把它当正文收录。
- 已发布文章 **100% 是 `public` / `paid_level=0`**；唯一一篇 `paid_level=2` 的文章是草稿。付费墙、402 预览、会员权益这条链在真实内容上**一次都没跑过**。
- `tiers` 两行价格齐全，但 `stripe_price_id` 为空——在线支付实际不可用；当前唯一能成交的是 `channel=manual` 的线下人工开通。
- 5 笔已支付订单全部为测试账号；`subscriptions` 只有 1 行且已取消。

**根因**：工程侧把「收款能力」当成了「商业闭环」；但闭环的另一半是**可售商品 + 真实需求**。

**代价**：所有关于转化率、客单价、LTV、归因收入的讨论都建立在 0 上；继续优化支付代码的边际收益接近 0。

**建议**：

1. 立刻处理 97 字符那篇：补全或转回 `draft`。
2. 选 1 篇最有价值的长文，设为 `paid_level>0`，用 manual 通道真实走一笔订单（可以是自己的另一个账号），把「付费 → 开通 → 到期」全链路在真实内容上跑通。
3. 明确 Stripe 的定位：要么配真实 `price_id`，要么在配置层显式禁用并在文档写清「当前仅 manual」。不要让「半配置」状态留在生产。

---

### R3　双份维护导致漂移（历史事故的共同根因）

**现象**：项目里有多处「手工镜像」，已经开始漂移。

**证据**：

- **权限码**：前端 79 个，后端 74 个。6 个只在前端：`content.pages.publish`、`content.products.publish`、`settings.manage`、`team.roles.manage`、`team.roles.view`、`team.users.view`；后端另有 1 个测试字符串 `anything.at.all`。没有任何测试断言两边集合相等。
- **前端数据层**：同时存在 `src/api/client.ts`、`src/api/cms/http.ts`、`src/api/cms/store.ts`（localStorage mock）三套访问方式；另有 **10 个文件、15 处裸 `fetch()`**（`m.tsx`、`site.tsx`、`read.$key.tsx`、`site-appearance.tsx`、`templates.tsx` 等），各自重复处理 token / 401 / 403。
- **mock 层静态打包**：`seed.ts`（132 行）与 `store.ts`（131 行）被 `index.ts` 无条件 import，即使 `CMS_MODE='real'` 也会进入生产 bundle。
- **文档与真实构建**：README 仍写 `./target/debug/cms-server.exe` 与 `cms_live.db`，而实际构建目录是 `tgt_build4`、运行库是 `cms.db`（`cms_live.db` 已是死文件）。

**根因**：两处需要人工同步的地方，缺少机械化校验。这与历史上 `inviteCode` 因 DTO 缺 `rename_all` 而静默丢失、付费墙字段被网关静默丢弃，是**同一类缺陷**——不是写错，而是「没有东西阻止它写错」。

**建议**：

1. 加一个后端测试：解析 `perm.rs` 与 `permissions.ts`，断言集合相等（排除通配符）。这是半小时的成本，能永久消除一类漂移。
2. 前端只保留一个 HTTP 入口；把 10 个文件里的 15 处裸 `fetch()` 逐步迁回 `client.ts`。mock 层改为动态 import，或直接删除（它服务的是「无后端演示」，而项目已经有真实后端）。
3. 修正 README，并加一条「构建/运行命令以 skill `admin-rust-build-handoff` 为准」的指针。

---

### R4　历史模型沉积在库里

**现象**：`0003_domain_model_v1` 建的 12 张表，有 9 张**零代码引用**，但仍带着数据留在 `cms.db`。

**证据**：

| 表 | 行数 | 代码引用 |
|---|---:|---:|
| `contents` | 13 | 0 |
| `channels` | 8 | 0 |
| `contexts` | 4 | 0 |
| `content_channels` | 4 | 0 |
| `content_contexts` | 3 | 0 |
| `sites` / `site_themes` / `themes` / `theme_versions` | 各 1 | 0 |

**代价**：

- 备份、迁移、审计时无法判断哪些是活数据。
- `contexts` 与未来可能引入的 Context 对象**同名不同义**，是明确的冲突源。
- 它是「先建对象、后找消费方会失败」的物证——留着它，下一次架构讨论还会被同一块石头绊倒。

**建议**：一次性决策——`DROP`，或统一改名到 `legacy_*` 并在文档标注「只读历史数据」。不要继续留在业务表命名空间里。

---

### R5　多租户是 schema 层的幻觉

**现象**：每张表都有 `tenant_id`，但运行时是硬编码。

**证据**：`main.rs:382` 直接 `tenant: "t_demo".into()`；没有从 host / header / token 解析租户的中间件；`tenants` 表只有 1 行；没有跨租户越权的测试。

**代价**：所有查询都带着一个**永远相同的维度**，读者会误以为系统支持多租户；真正要做时，又缺解析、隔离、迁移三块。这是「成本现在付、收益不确定」的典型。

**建议**：二选一——

- **真做**：加租户解析中间件 + 所有查询强制 scope + 至少一组越权测试；
- **认清单租户**：保留列（迁移成本高）但在架构文档明确写「单租户，`t_demo` 为保留常量」，并停止在新代码里扩散租户参数。

以当前阶段，**建议后者**。

---

### R6　单实例假设与可观测性都未落地

**现象**：系统在设计上依赖「只有一个进程」，但代码没有强制；出问题时也几乎没有结构化信号。

**证据**：

- 调度器是进程内 tick；SQLite 写入无跨进程协调；没有 leader lock / advisory lock。跑两个实例会双发布、双过期。
- 只有 CORS、BodyLimit、audit 三个 layer；**没有 TraceLayer、请求 ID、指标端点、错误聚合**。
- `events` 61 条全是 `article_view`，没有 `signup` / `order_paid` / `subscribe` 事件；`not_found_log` 只有 1 条测试数据。
- 无 CI；回归依赖本地 `cargo test`（全量约 14 分钟），反馈环偏长。

**代价**：一旦开始有真实流量，**无法回答「昨天为什么少了一单」**；也无法安全地水平扩容。

**建议**（按性价比排序）：

1. 先补事件：注册、下单、支付成功、订阅开通/到期，四个就够了。
2. 加请求 ID + 结构化日志（`tower-http` 的 `TraceLayer` 是现成的）。
3. 在调度器入口加一行「leader 行」检查（`UPDATE ... WHERE leader=...`），把单实例假设变成可执行的约束。
4. CI 至少跑 `cargo test` + `tsc --noEmit` + `vitest run`；慢测试拆分。

---

### R7　若干「语义超前」的设计

这些不是错误，但需要明确它们是**期权**而不是**功能**：

| 设计 | 现状 | 判断 |
|---|---|---|
| `agent.json` 的 `capabilities` | 全部是 `kind: "read"` 的只读端点 | 已用 `kind` 缓解误导，但命名仍建议改为 `data_endpoints`；内容站没有可执行能力，不必硬造 `invoke` |
| `articles.kind` | 全为 `post`，0 个 `problem` | 多态维度未被使用；`/problems` 是空体系 |
| `visibility` / `paid_level` / `price_points` | 已发布内容全为默认值 | 付费维度未被真实内容检验 |
| `blocks` 语义分块 | 未实现 | 对 GEO 的价值高于再加一个 `kind`，但应等有付费内容后再做 |
| GraphQL | 194 行，已接入 | 与 REST 并存，但当前无消费方；属于维护成本 |

**建议**：在架构文档里给每一项标上「已启用 / 期权 / 待验证」，避免下一次讨论把期权当既有能力。

---

## 5. 架构债务分级

| 级别 | 事项 | 触发条件 | 成本 |
|---|---|---|---|
| **P0** | 97 字符文章补全或下线 | 立即 | 小时级 |
| **P0** | 至少 1 篇付费文章 + manual 通道真实走单 | 立即 | 1 天 |
| **P0** | 权限码一致性测试 | 立即 | 0.5 天 |
| **P0** | 修正 README 的构建/库名 | 立即 | 10 分钟 |
| **P1** | 前端数据层收敛，移除生产 mock 打包 | 下一次前端迭代 | 2–3 天 |
| **P1** | 遗留表 DROP / 改名 `legacy_*` | 下一次迁移 | 0.5 天 |
| **P1** | 补 4 个生命周期事件 | 有真实流量前 | 1 天 |
| **P1** | 调度器 leader 锁 | 多实例部署前 | 0.5 天 |
| **P2** | CI（test + tsc + vitest） | 有第二位贡献者前 | 1 天 |
| **P2** | TraceLayer + 请求 ID + 指标 | 有真实流量后 | 1–2 天 |
| **P2** | 多租户真做 or 明确单租户 | 有第二个站点前 | 1–2 周 |
| **P2** | `blocks` 语义分块 | 有 3 篇以上付费内容后 | 3–5 天 |

---

## 6. 建议的 90 天路线（证伪优先）

原则：**先证明有人愿意付钱，再证明架构能扩张**。顺序不能反。

### 第 1–2 周：把商品做出来
- 补全 97 字符文章；把 4 篇长文做成可售商品（至少 1 篇 `paid_level>0`）。
- 用 manual 通道真实走一笔订单，验证「支付 → 开通 → 到期」在真实内容上成立。
- 明确 Stripe 状态：配置真实 `price_id`，或显式禁用。

### 第 3–4 周：把漂移关掉
- 权限码一致性测试；前端数据层收敛；README 修正。
- 遗留表清理（DROP 或 `legacy_*`）。
- 补 4 个生命周期事件，让漏斗有数据。

### 第 5–8 周：拿真实流量
- 用现有 4 篇长文做分发（搜索 / 社区 / 社交），目标是**非零的真实访客与注册**。
- 观察 `events` 里是否出现 `signup` / `order_paid`；观察 404 日志是否出现「同一意图的多种问法」。
- 只有出现非零转化，才讨论提高客单价/年付转化。

### 第 9–12 周：按证据决定扩张
- 若转化成立：再做 Agent Layer 的深化（`data_endpoints`、Evidence、对比页）与 `blocks`。
- 若转化不成立：问题在选题/定价/渠道，不在架构；此时加任何功能都无效。

---

## 7. 一句话总结

**这套架构已经足够支撑一个真实生意；它现在缺的不是能力，而是「被使用」。**

过去几个月的工程投入，把「万一要做」的部分都提前做完了；接下来最有价值的架构决策，是**克制**——把空表关掉、把双份维护机械化、把内容变成商品，让真实数据来决定下一个模块该不该存在。

---

## 附录：验证方法

```bash
# 代码规模
ls server/src/*.rs | wc -l
wc -l server/src/*.rs | tail -1
grep -cE '\.route\(' server/src/main.rs
grep -oE 'version: "[0-9]+_[a-z0-9_]+"' server/src/db.rs | tail -20

# 前端规模
find web/src/routes -name '*.tsx' | wc -l
find web/src -name '*.ts' -o -name '*.tsx' | xargs wc -l | tail -1

# 数据库真相（只读）
python -c "
import sqlite3
c=sqlite3.connect('server/cms.db')
for t in ['members','orders','subscriptions','events','articles','tiers']:
    print(t, c.execute(f'select count(*) from {t}').fetchone()[0])
"

# 权限码漂移
python - <<'PY'
import re, io
fe=set(re.findall(r"'([a-z][a-z0-9_]+(?:\.[a-z0-9_]+)+)'", io.open('web/src/config/permissions.ts',encoding='utf-8').read()))
be=set(re.findall(r'"([a-z][a-z0-9_]+(?:\.[a-z0-9_]+)+)"', io.open('server/src/perm.rs',encoding='utf-8').read()))
print('frontend', len(fe), 'backend', len(be))
print('frontend-only', sorted(fe-be))
print('backend-only', sorted(be-fe))
PY
```

### 关键文件索引

| 主题 | 文件 |
|---|---|
| 订阅状态机 | `server/src/sub_state.rs` |
| Agent 事实源 | `server/src/agent_layer.rs` |
| CRUD 网关 | `server/src/resources.rs` |
| 权限矩阵 | `server/src/perm.rs` ↔ `web/src/config/permissions.ts` |
| 迁移账本 | `server/src/db.rs::MIGRATIONS` |
| 前端数据层 | `web/src/api/client.ts`、`web/src/api/cms/{http,store,index}.ts` |
| 主页渲染 | `F:\project\coucouya` + `server/templates/coucouya/` |
| 历史架构报告 | `docs/AGENT_GROWTH_REPLAN_2026-09-22.html`、`docs/SUBSCRIPTION_STATE_MACHINE_2026-09-28.html` |
