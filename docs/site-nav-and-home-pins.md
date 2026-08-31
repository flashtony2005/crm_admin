# 导航链接 + 主页置顶：建表落地说明

日期：2026-08-29
范围：nav_links 表（导航/页脚链接管理）+ articles `?limit=` + home_pins 表（主页置顶精确编排）

---

## 一、改动总览

| 文件 | 改动 |
|---|---|
| `server/src/db.rs` | 新增 `nav_links`、`home_pins` 两张表 DDL；启动时幂等迁移（home JSON 的 nav/footer links 导入 nav_links，表非空即跳过） |
| `server/src/perm.rs` | 新增 `site.nav.*` / `site.homePins.*` 权限码；Editor 可写、Viewer 只读 |
| `server/src/resources.rs` | 注册 `navlinks`、`home_pins` 两个 TableDef（自动获得 `/api/navlinks`、`/api/home_pins` CRUD） |
| `server/src/public_api.rs` | articles 支持 `?limit=`（默认 100，上限 200）；新增 `GET /api/public/nav`、`GET /api/public/home-pins?slot=` |
| `server/src/main.rs` | 注册 `/api/public/nav`、`/api/public/home-pins` 路由 |
| `web/src/config/permissions.ts` | 前端权限码镜像（与后端矩阵严格一致） |
| `web/src/routes/settings.tsx` | 「站点外观」新增两个行内编辑器：①「导航与页脚链接」（增删改/排序/启停/新窗口）；②「主页置顶文章」（选文章置顶/排序/启停/移除，按 slot 分「代表文章」「系列」两组）。列表均按 (grp/slot, sort) 重排，解决通用网关 updated_at DESC 的顺序问题 |

## 二、新表结构（与用户方案的两处修正）

```sql
-- nav_links：grp 分 nav | footer 两组
-- 修正1：UNIQUE 由 (tenant_id, grp, label) 改为 (tenant_id, grp, href)
--        —— label 是展示文本，同组可能同名；防重应落在目标地址
CREATE TABLE IF NOT EXISTS nav_links (
  id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
  grp TEXT NOT NULL DEFAULT 'nav',
  label TEXT NOT NULL DEFAULT '', href TEXT NOT NULL DEFAULT '',
  target TEXT DEFAULT '',                       -- '' 同窗口 | _blank 新窗口
  sort INTEGER DEFAULT 0, enabled INTEGER DEFAULT 1,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  UNIQUE(tenant_id, grp, href));
CREATE INDEX IF NOT EXISTS idx_nav_links_grp ON nav_links(tenant_id, grp, sort);

-- home_pins：主页置顶精确编排
CREATE TABLE IF NOT EXISTS home_pins (
  id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL,
  slot TEXT NOT NULL DEFAULT 'writing',         -- 展示位：writing / series
  article_id TEXT NOT NULL,
  sort INTEGER DEFAULT 0, enabled INTEGER DEFAULT 1,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  UNIQUE(tenant_id, slot, article_id));
```

## 三、公开 API

### GET /api/public/nav（免认证）
```json
{ "ok": true, "data": { "nav": [{"id","label","href","target"}], "footer": [...] } }
```
仅 `enabled=1`，按 `(grp, sort)` 升序。5199 公开站的 SiteNav/SiteFooter 读取顺序建议：**表 → home JSON → 内置**（现有 home JSON 的 nav/footer 键保留兼容，不删除）。

### GET /api/public/home-pins?slot=writing（免认证）
JOIN articles，仅返回 `enabled=1` 且 `status='published'` 的文章，按 `pin.sort` 升序：
```json
{ "ok": true, "data": [ { "pinSort", "id", "title", "slug", "summary", "featured_image", "published_at", "tags", "updated_at" } ] }
```
前端策略：pins 有配置用 pins；无配置回落 `?featured=1` 过滤。

### GET /api/public/articles 新增 ?limit=
`?limit=` 控制返回条数（默认 100，clamp 1–200）。首页「显示条数」配置由 lab 区块 JSON 下发，前端读配置后拼该参数。

## 四、一次性迁移（已内置，无需手动执行）

`db.rs` bootstrap 在 **nav_links 表为空** 时，从 `site_settings.home` JSON 读取 `{nav,footer}.links[]`，逐条 INSERT（label/href/target/sort），`INSERT OR IGNORE` 保证幂等。当前 cms.db 实测：footer 3 条（X/KOSX.ai/平台风险提示）将被导入，nav 0 条跳过。

## 五、本地编译与验证步骤

```bash
# 1. 编译后端（沙箱无法编译，需本机执行）
cd server && cargo build
#   启动后观察日志：无「建表失败」即新表就绪

# 2. 前端（本机已可通过 tsc 校验）
cd web && pnpm install && pnpm build   # tsc -b 已通过

# 3. 接口验证（沙箱内可执行，服务需用户本机启动）
bash server/verify_nav_homepins.sh
# 或逐条：
curl http://localhost:8088/api/public/nav
curl "http://localhost:8088/api/public/home-pins?slot=writing"
curl "http://localhost:8088/api/public/articles?featured=1&limit=5"

# 4. 后台操作验证（浏览器 → 侧边栏「站点外观」，路径 /site-appearance）
#    导航与页脚链接：顶部导航/页脚两组，增删改、▲▼排序、启停开关、新窗口勾选
#    主页置顶文章：代表文章/系列两组，下拉选已发布文章 → 置顶；▲▼排序、启停、移除

# 5. 权限抽查
#    editor 登录应能改导航与置顶；viewer 登录只能看
```

### 「站点外观」已提升为独立菜单页
原「设置 → 站点外观」tab 已移除，改为一级菜单项（侧边栏，路径 `/site-appearance`）：

| 文件 | 改动 |
|---|---|
| `web/src/routes/site-appearance.tsx` | 新页面（路由 `/site-appearance`），承载主题/模板/品牌/主页风格、首页区块、导航链接、主页置顶 |
| `web/src/routes/settings.tsx` | 移除 appearance tab 及其全部组件代码（1448 → 612 行），只保留 profile/preferences/security 三个 tab |
| `web/src/config/nav.ts` | 新增菜单项「站点外观」，`perm: 'site.nav.view'` |
| `web/src/components/layout/Sidebar.tsx` | 图标库补 `palette`（缺失会 fallback 成圆点） |
| `web/src/hooks/useTabOpener.ts` | 补 `/site-appearance` → 「站点外观」标签标题（否则标签页显示英文路径） |

页面内各区块按权限独立显隐：主题/模板/品牌需 `site.settings.update`（Owner）；导航与置顶需 `site.nav.view`，**Editor / Viewer 也能进**——这解决了此前「Editor 有 `site.nav.*` 写权限却因 tab 仅 Owner 可见而没有入口」的问题。

> 新增 TanStack Router 文件路由后，磁盘上的 `src/routeTree.generated.ts` 不会自动更新（dev server 已运行时尤其如此），`tsc` 会报 `not assignable to parameter of type 'keyof FileRoutesByPath'`。触发生成的方式：重启 dev server，或用 Vite Node API 起一个临时实例（另选端口，几秒后关闭）让 `TanStackRouterVite` 插件重写路由树。

### 判断「8088 跑的是不是新版」
新版接口未注册时，请求会被通用资源网关的 `/api/{table}/{id}` 截获：
```bash
curl -s http://127.0.0.1:8088/api/public/nav
# 旧版（路由未注册）：{"error":"未登录","ok":false}   ← table=public, id=nav 被当成资源读取
# 新版：{"ok":true,"data":{"nav":[...],"footer":[...]}}
```

## 六、首页区块：JSON 文本域 → 结构化表单

原 `BlockCard` 用一整个 `<textarea>` 编辑 body 的原始 JSON，运营人员必须懂 JSON 语法（错一个逗号就保存失败）。已改为**按值类型递归渲染的结构化表单**。

### 方案选型（三选一）

| 方案 | 做法 | 评价 |
|---|---|---|
| A. 手写专用表单 | 为 about/org/lab/web3 各写一套 schema 表单 | UX 最精确，但硬编码 schema，新增区块要写新组件 |
| B. 引入 JSON Schema 表单库 | @jsonforms / react-jsonschema-form | schema 驱动很专业，但新增数百 KB 依赖 + 样式适配成本 |
| **C. 自研结构推导表单（采用）** | `JsonForm` 按值类型递归渲染 | **零依赖、对任意 JSON 通用、新增区块自动适配、中文标签靠字典覆盖** |

选 C 的关键理由：首页区块结构不固定（以后可能加区块），按值推导新旧通吃，且区块结构不深（对象 + 一层数组），递归渲染完全够用。

### `web/src/components/cms/JsonForm.tsx` 能力

- **object** → 字段分组，标签取 `LABEL_ZH` 中文映射（未命中则显示英文 key），可增删字段
- **array** → 列表，可 ▲▼ 排序、删除；「+ 添加一项」按上一项结构**克隆空壳**（不复制内容）
- **string** → 单行输入；长度 > 60 或含换行自动切多行文本域
- **number** → 数字输入；**boolean** → 开关；**null** → 空值可填
- **图片字段**（logo/cover/image/avatar…）→ 附加「素材库」按钮，懒加载 `/api/media` 点选填入，并带 32px 预览
- `BlockCard` 侧保留可折叠的「查看原始 JSON」作高级逃生舱（只读展示）

已覆盖的中文标签：`kicker/眉标`、`tagline/标语`、`hero/主视觉`、`ctaLabel/按钮文字`、`affiliation/所属组织`、`eyebrow/小标题`、`pillars/支柱`、`items/条目`、`writing/代表文章`、`series/系列`、`disclaimer/免责声明`、`inviteCode/邀请码` 等 28 个（见 `LABEL_ZH`）。新增字段只需往字典里加一行。

### 测试
`src/components/cms/__tests__/JsonForm.test.tsx`（4 项，全通过）：中文标签渲染、叶子编辑不影响同级字段、数组上移交换顺序、新增项克隆空壳、图片字段附素材库入口。

## 七、注意事项

1. **通用网关列表排序**：`/api/navlinks` 按 `updated_at DESC` 返回，管理页前端已按 `(grp, sort)` 重排；如需后端直接排序，可后续给 TableDef 加 `order_by` 字段（需重编译）。
2. **home JSON 保留**：旧 `nav.links / footer.links` 键不删除，供 5199 旧逻辑回落；后台 JSON 编辑入口仍在（区块卡片）。
3. **home_pins 后台编辑器**：与 NavLinksEditor 同构。注意通用 CRUD **不联表**，所以组件自行拉取 `/api/articles` 把 `articleId` 映射为标题；新增时前端会校验 `UNIQUE(tenant_id, slot, article_id)`，同展示位重复置顶同篇会直接提示。
4. **权限命名**：采用 `site.nav.*` / `site.homePins.*`（site 域），与 `site.settings.update` 命名体系一致；未采用 content 域。
5. **`Col::Bool` 传值**：`enabled` 列在 resources.rs 里是 `Col::Bool`，POST/PUT 必须传 JSON 布尔（`true`/`false`），传数字 `1` 会被存成 `0`。
6. **沙箱内无法编译/启动**：环境拒绝「打开已存在文件用于写入」（`.fingerprint`、遗留 `.cargo-lock`），且服务在沙箱内启动会因 cms.db 只读而崩。改完 Rust 源码请双击 `server/build_sections.cmd` 完成编译 + 重启。
