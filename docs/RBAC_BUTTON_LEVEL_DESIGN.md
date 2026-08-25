# RBAC 按钮级权限设计 — 借鉴 RuoYi，落地到 AI-Native CMS

> 状态：Phase 1 前端层已实现并验证（2025-08）。后端 Axum 层为 Phase 1 后端的验收项。
> 关联：[PRODUCT_VISION.md](./PRODUCT_VISION.md) §7 安全链路、G4「AI 不能越权」。

---

## 一、RuoYi 是怎么做的（调查结论）

### 1.1 数据模型：一张 `sys_menu` 表，四种节点

| menu_type | 含义 | 例子 | 携带 perms |
|---|---|---|---|
| M | 目录 | 系统管理 | 否 |
| C | 菜单页面 | 用户管理 | 否 |
| F | **按钮** | 新增用户 | `system:user:add` |

- 角色 ↔ 菜单（含 F 按钮）通过 `sys_role_menu` 多对多勾选；用户 ↔ 角色通过 `sys_user_role`。
- 权限标识命名约定：`模块:资源:动作`（`system:user:add`）。
- ⚠️ 官方手册未强调但实践中的坑（见 [CSDN: 若依框架的按钮权限](https://blog.csdn.net/qq_42701659/article/details/132759325)）：
  **F 按钮节点要挂在父菜单(M)下而不是子菜单(C)下**，否则角色分配界面的树结构错乱。

### 1.2 运行时链路

```text
登录 → 聚合该用户所有角色的 perms 成 Set<String> 存入会话
     ↓
前端：v-hasPermi="['system:user:add']" 自定义指令（无权限 = 移除 DOM）
      checkPermi(['...']) 工具函数（v-if 场景）
     ↓
后端：@PreAuthorize("@ss.hasPermi('system:user:add')")   ← RuoYi-Vue (Spring Security)
      @RequiresPermissions("system:user:add")            ← 原版 RuoYi (Shiro)
      支持 logical = AND / OR，'*:*:*' 全量通配
```

来源：[RuoYi 官方后台手册·权限注解](https://doc.ruoyi.vip/ruoyi/document/htsc.html)、
[v-hasPermi 前后端实现梳理](https://blog.csdn.net/m0_74824823/article/details/144637728)、
[若依权限管理设计（阿里云社区）](https://developer.aliyun.com/article/1419213)

### 1.3 数据权限（行级，另一维度）

`@DataScope(deptAlias, userAlias, permission)` 通过 SQL 动态拼接实现
全部 / 自定义 / 本部门 / 部门及以下 / 仅本人 五档；`permission` 参数用于多角色场景
精确指定本次操作哪个角色的数据范围生效。（Phase 4 Customers/Leads 的租户隔离会用到此思想）

### 1.4 RuoYi 的局限（我们的机会）

- 权限串靠人工约定，散落在 DB 和代码两处，拼写错误运行时才暴露；
- 无审批 / 审计概念 —— 高风险操作只有"有权限/没权限"，没有"AI 先干、人再批"；
- v-hasPermi 直接移除 DOM，用户不知道"为什么没有这个按钮"。

---

## 二、我们的方案：权限码 = Capability Code，一份码表三方共用

RuoYi 的 `perms` 字符串 ≈ 我们架构里已有的 **Capability**。因此不引入新概念：

```text
                    ┌─ 前端按钮显隐（<Auth perm> / usePermission）
capability code ────┼─ 后端接口校验（Axum extractor，403 → client.ts 统一 toast）
（如 content.articles.create）
                    └─ AI 能力边界（Policy 链路：editor 下 content.publish → 转审批）
```

这直接满足产品纲领 G4：**AI 是执行者，不是超级管理员** —— AI 的 publish 能力在
editor 角色下被同一份码表拦截，转入 Approvals 流程等 Owner 批准。

## 三、已实现（Phase 1 前端层）

| 模块 | 职责 |
|---|---|
| `src/config/permissions.ts` | 强类型码表 `P`（编译期防拼写错误）+ owner/editor/viewer 权限矩阵 + 通配匹配 |
| `src/store/permission.ts` | zustand 权限状态；`granted` 字段是后端接入的唯一切换点（服务端下发优先，本地矩阵兜底） |
| `src/hooks/usePermission.ts` | `has / hasAny / hasAll`（对应 RuoYi logical OR/AND） |
| `src/components/cms/Auth.tsx` | `<Auth perm mode>` 组件：`hide`（默认，对应 v-hasPermi）/ `disable`（置灰+tooltip，保留可发现性 —— 比 RuoYi 的静默移除更好） |
| 导航接线 | **权限锚定导航**：`nav.ts` 每个入口挂准入权限码（`perm` 字段），`filterNavByPerm(nav, has)` 按 `usePermission().has` 过滤 —— 菜单可见性从权限矩阵**派生**，单一事实源，不会出现"菜单可见但按钮无权"。管理员(owner)持全量码看全部菜单；经办者(editor)只见 Home / Content / AI(Assistant+Tasks) / Business；Automation、Team、Settings、Approvals 自动隐藏（分组全灭时整组消失）。锚定码选"代表性动作"而非查看权：Approvals 锚 `ai.approvals.decide`（Owner 裁决台）、Automation 锚 `*.toggle`（经营配置） |
| 页面接线 | Articles/Pages/Products/Media/Customers/Leads/Approvals/Workflows/Integrations/Team 共 20+ 个操作点 |

**关键矩阵语义**（与纲领 §7 严格一致）：

- `owner`：`*` 全量（含发布、审批裁决 ai.approvals.decide、团队管理）→ 看全部菜单
- `editor`：内容增删改 ✅、**发布 ❌**（content.*.publish 刻意不授予）、审批裁决 ❌；菜单仅见与其职责相关的 Home/Content/AI(Assistant+Tasks)/Business
- `viewer`：只读 + AI 助手可用；菜单同 editor（页面内无任何写按钮）

⚠️ 教训（已写入测试）：通配 `'content.articles.*'` 会连 publish 一起授予 ——
**本地矩阵一律显式枚举写权限，通配仅用于服务端下发聚合场景。**

验证：vitest 158/158（矩阵 / Auth / store / 导航过滤）；Playwright 三角色冒烟 11/11（含菜单可见性断言）。

## 四、后端计划（Axum + SeaORM，Phase 1 后端验收项）

```rust
// 权限表沿用 RuoYi 思想但简化为三张表（无菜单树，导航由前端持有）
// role        (id, key, name)
// permission  (id, code UNIQUE)             -- 即 capability code
// role_permission (role_id, permission_id)

// 接口校验 = 一个 extractor，等价 @PreAuthorize("@ss.hasPermi(...)")
async fn create_article(
    RequirePerm("content.articles.create")[: actor]: RequirePerm,
    State(db): State<Db>, Json(input): Json<CreateArticle>,
) -> ApiResult<Article> { ... }
// 无权限 → 403 {"ok":false,"error":"需要权限：content.articles.create"}
// 前端 api/client.ts 已有 403 toast 处理，零改动

// 登录后 GET /api/auth/me 返回：
// { user, roles: ["owner"], permissions: ["*", "content.articles.create", ...] }
// 前端 setGranted(perms) 即完成切换，ROLE_PERMS 矩阵退役为测试夹具
```

数据权限（行级）：Phase 4 给 customers/leads 增加 `tenant_id` 作用域列 +
查询层自动注入（借鉴 @DataScope 思想，单租户 MVP 先按"仅本人/本企业"一档实现）。

## 五、Phase 3 展望：可视化权限编辑器

Team/Roles 页扩展为勾选矩阵（角色 × 权限码），对应 RuoYi 的角色管理界面；
AI 能力（content.publish 等）同样出现在矩阵中，勾选行为对人和 AI 同时生效 ——
这是「Permission → Capability → Policy」链路的可视化管理面。
