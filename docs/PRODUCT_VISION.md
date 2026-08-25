# PRODUCT_VISION — AI-Native Small Business CMS

> 状态：**已确认的产品纲领（Canonical）**。后续所有开发以此文档为基准。
> 任何与本文件冲突的旧文档（Ontology OS / Admin Framework 系列 ADR）自动降级为历史参考。

---

## 一句话目标

让没有专业 IT 团队的小企业，也能用一个简单的系统建立网站、管理内容、获取客户，
并让 AI 自动完成大量日常数字运营工作。

**最终一句话**：AI-Native Small Business CMS —— 一个让小企业用自然语言管理网站、内容、
客户和日常数字运营工作的 AI 工作台。

---

## 1. 产品定位

### 是什么

面向小企业的 **AI 数字工作台**：

```text
网站 + 内容 + 客户/线索 + 团队 + AI + 自动化
```

### 不是什么

- 不是功能很多的 CMS / Admin Framework
- 不是传统 ERP 后台

### 内部技术与用户视角的边界

用户**不需要理解**底层的：Capability、Policy、Plugin Runtime、Agent Runtime、Event。

用户**看到的只有**：Home、Content、AI、Business、Automation、Team、Settings。

---

## 2. 五个产品目标

| # | 目标 | 要点 |
|---|------|------|
| G1 | **5 分钟完成企业初始化** | 选择行业模板（餐饮/咨询/教育/房地产/律所/本地服务/电商），系统自动生成页面、内容结构、导航、表单、AI 配置、基础工作流 |
| G2 | **普通员工无需培训就能使用** | 核心操作只有 Pages / Articles / Products / Media / Customers |
| G3 | **老板用自然语言让 AI 做事** | 例："把新品 X 发布到网站，并写一篇介绍文章。" → 搜索 → 读取 → 生成 → 修改 → 审核 → 发布 |
| G4 | **AI 可以工作，但不能越权** | 统一链路：Permission → Capability → Policy → Approval → Action → Audit |
| G5 | **低成本** | Rust/Axum/SeaORM/Turso + React/HeroUI/TanStack；不提前引入 Redis/Kafka/K8s/微服务/Elasticsearch/Vector DB |

---

## 3. 产品信息架构（用户视角）

### Desktop（管理中心）

```text
Home
Content
  Pages / Articles / Products / Media
AI
  Assistant / Tasks / Approvals
Business
  Customers / Forms / Leads
Automation
  Workflows / Integrations
Team
  Users / Roles
Settings
```

职责：深度内容管理、产品管理、团队管理、复杂设置、数据表、Workflow、Plugin。

### Mobile（AI 工作助手）

```text
Home / AI / Content / Inbox / Me
```

- 定位：**不是把 PC 后台缩小到手机**。PC 是管理中心，手机是 AI 工作助手。
- 强化：AI Assistant、Push Notification、Approval、Inbox、Voice、Quick Action。
- 典型场景：
  - "帮我看看今天有没有重要客户。" → 发现 3 个高优先级客户 → 生成摘要 → 给出回复建议
  - "把昨天的新产品发布掉。" → 检查内容 → SEO → 风险检查 → 请求批准 → 老板点击"批准"

### Mobile 技术方案（第一阶段）

- React Responsive + **PWA**（不马上 React Native）
- 结构：React ├── Desktop Layout └── Mobile Layout
- 共享：API、Auth、Permission、AI、Workflow、Query

---

## 4. 技术架构

### 后端

```text
Rust + Axum + SeaORM + Turso + JWT + RBAC

Axum ↓ Service ↓ Capability ↓ Policy ↓ SeaORM ↓ Turso
```

### 前端

```text
React + HeroUI + TanStack + Tailwind + Tiptap
```

TanStack 使用清单：Router、Query、Table、Form、Virtual、Hotkeys。

设计方向：Linear / Vercel / Supabase / Raycast 风格，**不是传统 ERP 后台**。

---

## 5. ⛔ 关键技术决策记录（ADR 级别，已由 Owner 确认）

### TD-1：不走"本体建模"路线

- **决策**：产品**不采用** ODL / ObjectType / LinkType 本体建模内核作为地基。
- **理由**：小企业 CMS 需要的是稳定、可预测的固定结构（Pages/Articles/Products/Media），
  第一阶段"不过度动态化"。本体内核是为动态建模问题设计的，引入它 = 提前支付灵活性成本。
- **执行**：
  - Phase 1 直接使用 **SeaORM 固定表结构**；
  - ontology kernel 已从仓库移除（2025 清理）；`oldsrc/` 仅作 RuoYi 参考代码，**不编译进产品、不迁移其代码**；
  - 可借鉴的只有语义与测试思路（RBAC/Policy/Capability 的行为定义）。

### TD-2：ContentType 预留方案

- 第一阶段不做动态 Schema。预留扩展点：未来若需要自定义内容类型，
  **优先用 JSON 字段 + 校验层**实现，届时再评估是否需要更重的方案。

### TD-3：旧代码复用规则

- 沿用重写铁律：不复制 `oldsrc/` 任何代码，只提取已验证的语义（如 Editor 不能直接发布的
  Policy 规则形态、Capability 命名约定）。

### TD-4：不提前引入的基础设施

Redis / Kafka / Kubernetes / 微服务 / Elasticsearch / Vector DB —— 只有真正遇到瓶颈才考虑。

---

## 6. CMS 核心（第一阶段范围）

先做固定的六类：**Pages、Articles、Products、Media、Categories、Tags**。

内容流程：

```text
Content ↓ Version ↓ Workflow ↓ Publish
```

---

## 7. AI 是产品主入口

顶部常驻输入框：

> ✦ 让 AI 帮你完成工作……

### 第一阶段 AI 能力（只做真正有价值的）

核心内容能力：

```text
content.search / content.read / content.create / content.update / content.publish
```

增值能力：SEO、Translation、Summary、Marketing Copy、Customer Reply。

### AI 安全架构（项目最重要的技术设计之一）

```text
Human ↓ Role ↓ Permission ↓ Agent ↓ Capability ↓ Policy ↓ Risk Check ↓ Approval ↓ Action ↓ Audit
```

示例：

- **Editor** → AI 可以修改草稿；AI **不能**直接发布。
- **Owner** → AI 可以请求发布；高风险操作仍可要求审批。

原则：**AI 是执行者，不是超级管理员。**

---

## 8. Plugin 系统（对用户隐藏复杂性）

- 用户看到：**Apps / Integrations**（不是 Plugin Runtime）。
- 优先集成：SEO、Google Analytics、Search Console、Email、WhatsApp、CRM、Stripe、Shopify。
- 内部：Plugin ↓ Capability ↓ Policy ↓ Event。
- 第一版不需要复杂的 Marketplace。

## 9. 缓存方案（第一阶段）

```text
Turso
 ├── Business Tables
 └── cache_entries
```

以后按需演进 L1 Memory → L2 Turso → Database；只有遇到性能瓶颈才考虑 Redis/Valkey。

## 10. Agent 分工（内部概念，用户不可见）

| 角色 | 定位 | 职责 |
|------|------|------|
| **Pi** | Business Agent | 内容、SEO、翻译、营销、日常运营 |
| **DSH** | Agent/Harness Adapter | 借鉴其插件化、Agent、Session、Tool、Event 思路 |
| **Codex** | Developer Agent | 改 Rust、改 React、测试、迁移、开发 Plugin |

---

## 11. MVP 路线图（严格按此顺序）

```text
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5
```

### Phase 1 — 地基（登录 + 内容）

- **交付物**：登录、RBAC、企业（多租户基础）、Pages、Articles、Products、Media。
- **验收标准**：
  - [ ] 三种角色（Owner/Editor/Viewer）登录后看到不同权限的操作；
  - [ ] 四类内容的增删改查 + 版本记录；
  - [ ] Axum + SeaORM + Turso 单后端可本地一键启动；
  - [ ] 无任何 AI 功能依赖（AI 完全缺席也能用）。

### Phase 2 — AI Assistant

- 交付物：AI 侧边栏/顶栏入口、content.* 五个能力、生成/修改、SEO、翻译。
- 验收标准：
  - [ ] 自然语言指令能走通 Capability → Policy → Action → Audit 全链路；
  - [ ] Editor 角色下 AI 无法直接发布（被 Policy 拦截并可审计）。

### Phase 3 — Workflow / Approval / Audit / Team

- 交付物：审批流、审计查询、团队成员与角色管理界面。
- 验收标准：
  - [ ] 手机端能收到审批推送并一键批准/驳回；
  - [ ] 所有 AI 操作可在 Audit 中追溯（谁、何时、做了什么、谁批准）。

### Phase 4 — Customers / Forms / Leads / Integrations

- 交付物：客户管理、表单收集、线索流转、第一批 Integrations（Email/GA/Search Console）。

### Phase 5 — Plugin / Agent / Automation

- 交付物：Plugin 运行时（Capability+Policy+Event）、Automation 工作流编辑器。

### 明确不做（任何阶段都不提前做）

Multi-Agent 协作编排、World Model、Self-Evolving Agent、复杂 WASM ABI、Kafka、微服务。

---

## 12. 核心竞争力

不是 Rust，不是 HeroUI，甚至不只是 AI。真正的竞争力：

1. **极简** — 小企业不用学习复杂 CMS
2. **AI 真正执行** — 不是聊天，而是：理解 → 操作 → 审批 → 完成
3. **安全** — Permission + Capability + Policy + Audit
4. **低成本** — Local-first、SQLite/Turso、单后端
5. **可扩展** — 通过 Plugin / Integration / Agent 逐步扩大能力

产品上只追求一件事：

> **让老板少操作后台，让 AI 多完成工作，同时保持企业数据和权限可控。**
