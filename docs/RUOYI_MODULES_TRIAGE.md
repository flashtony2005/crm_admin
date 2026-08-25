# RUOYI_MODULES_TRIAGE — RuoYi 系统模块对照裁决

> 背景：以 RuoYi 完整模块清单为参照，逐项决定「AI-Native Small Business CMS」的取舍。
> 总原则（PRODUCT_VISION）：我们**不是**大而全 Admin Framework；用户只看
> Home / Content / AI / Business / Automation / Team / Settings 七个入口；
> 内部复杂性（Capability/Policy/Plugin）对用户不可见；低成本，不提前引入基础设施。
>
> 结论先行：**21 个模块 → 采纳改造 3 · 变形合并 7 · 缓做 1 · 拒绝 10**。
> 所有采纳项全部落入现有七个入口，顶层导航零新增 —— 产品形态守住了。

---

## 一、系统管理

| RuoYi 模块 | 裁决 | 我们的形态 | 落点 / 阶段 |
|---|---|---|---|
| 用户管理 | ✅ 采纳改造 | 成员管理：邀请、停用、角色指派、最近活跃。小企业不需要 RuoYi 的全套用户字段 | `/team/users`（mock 版已实现）· Phase 3 接后端 |
| 角色管理 | ✅ 采纳改造 | 角色 × 权限码矩阵（含 **AI 能力边界**展示，RuoYi 没有）＋权限预览切换器；Phase 3 升级为可视化勾选矩阵 | `/team/roles`（已实现矩阵视图）· Phase 3 |
| 菜单管理 | ⛔ 拒绝的是「菜单配置界面」，**不是**「按角色显示不同菜单」的能力 | 后者已实现且冒烟验证：`nav.ts` 节点带 `roles` 字段 → `filterNavByRole()` 按 `store/permission` 当前角色过滤 Sidebar（editor/viewer 无 Team 区）。区别在哲学：RuoYi 把菜单存库、管理员可视化配置、权限授予即菜单勾选；我们视菜单结构为产品设计（发版才变），权限以动作码为中心，菜单可见性是派生结果。若未来出现白标/模板级 IA 差异，加 `customer_nav_override(key, visible)` 一张表即可升级，现阶段属过度设计 | 已实现 · 见 [RBAC_BUTTON_LEVEL_DESIGN.md](./RBAC_BUTTON_LEVEL_DESIGN.md) |
| 部门管理 | ⏸ 缓做 | 小企业组织扁平，树形机构是中大型需求。若出现连锁多门店，以「门店 Store」实体进入模型，而非部门树 | Phase 5+；旧 `organization/department` 仅留参考 |
| 岗位管理 | ⛔ 拒绝 | User 上一个职位文本字段即可，不配拥有独立管理页 | — |
| 字典管理 | ♻️ 变形合并 | 不做通用字典后台：①固定枚举（内容状态/线索状态）直接写在代码；②业务可变项 = Content 的 Categories/Tags（纲领 §6 已规划）＋ Settings 少量自定义选项 | Categories/Tags · Phase 1 后半 |
| 参数设置 | ✅ 采纳改造 | 企业设置：营业时间、联系方式、AI 配置开关。系统级参数走环境变量，不进 UI | `/settings` · Phase 2-3 充实 |
| 通知公告 | ♻️ 变形合并 | 不做"公告管理"；通知统一进 **Inbox**（Mobile 五入口之一，纲领 §11），公告只是其中一种消息类型 | Inbox · Phase 4 |
| 日志管理（操作/登录） | ♻️ 变形合并 | 不做两个独立日志页。纲领安全链路末端就是 **Audit**：一个统一审计视图，三个过滤器（AI 操作 / 人工操作 / 登录事件），每条记录可追溯到人、时间、批准链 | Audit · Phase 3；旧 `monitor/oper-log`、`login-log` 仅参考模式 |

## 二、系统监控

| RuoYi 模块 | 裁决 | 理由与替代 |
|---|---|---|
| 在线用户 | ⛔ 拒绝独立页 | 运维视角对小企业无意义；Team 成员列表的"最近活跃"字段（已实现）覆盖真实需求 |
| 定时任务 | ♻️ 变形合并 | 成为 Automation Workflows 的触发器引擎（"每周一 08:00""客户创建时"这类自然语言预设），**绝不暴露 cron 表达式**；执行结果即 Workflow 运行记录 | 
| 数据监控（Druid/SQL） | ⛔ 拒绝 | 无 Druid、无自建连接池；Turso 平台侧有查询指标 |
| 服务监控（CPU/内存） | ⛔ 拒绝 | 单后端托管平台自带指标；后端提供 `/healthz` 即可（低成本原则 TD-4） |
| 缓存监控 / 缓存列表 | ⛔ 拒绝（现阶段） | 第一阶段无 Redis；`cache_entries` 表（纲领 §9）出现性能需求时再谈 |

## 三、系统工具

| RuoYi 模块 | 裁决 | 理由与替代 |
|---|---|---|
| 表单构建 | ♻️ 变形合并 | Phase 4 Forms 做**面向用户的简单表单编辑器**（预设字段类型拼装），而不是生成 HTML 代码的开发工具 |
| 代码生成 | ⛔ 拒绝进产品 | 开发者工具。我们的对应物是 **Codex Developer Agent**（纲领 §10）：开发期由 Agent 生成 CRUD 与迁移，产品界面里永远不存在这个功能 |
| 系统接口（API 文档） | ⛔ 拒绝进产品 | 开发期维护 OpenAPI spec 并生成前端类型（web 已有 openapi-typescript 工作流），不对用户暴露 |

---

## 四、对旧代码的处置

`oldsrc` 与 `demo/web/src/routes/{users,roles,menus,dict,monitor,sys,organization,tenants}` 是这份清单的历史实现，
按重写铁律**不迁移代码**；若重建采纳项（如统一 Audit 视图），只提取其交互语义与测试思路。
这些路由当前从新导航不可达，待 Phase 3 对应功能落地后再物理清理。

## 五、给后续开发的约束

1. 任何新功能先问：它落在七个入口的哪一个？答不上来 = 大概率不该做（或该并入既有入口）；
2. 本清单中的"拒绝"项，除非产品定位变更，不再讨论；
3. "变形合并"项的实现顺序跟随 MVP Phase 1-5（纲领 §11），不因 RuoYi 有而提前。
