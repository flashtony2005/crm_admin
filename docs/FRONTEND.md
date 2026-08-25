# 前端说明（web/）

React 19 + TypeScript + Vite，TanStack 全家桶（Router/Query/Table/Form）+ HeroUI + Zustand + i18next。

---

## 启动与模式

```bash
cd web
npm install
npm run dev          # 默认 mock 模式，端口 5188
VITE_CMS_MODE=real npm run dev   # 接真实后端（vite 代理 /api → :8088）
```

- `mock`：localStorage 适配器，开箱演示，无需后端。
- `real`：`httpCollection` 直连 Axum 后端，页面代码零改动。
- 测试：`npm test`（Vitest 单元/组件）、`npm run build`（tsc + vite build）。

---

## 路由（信息架构）

| 分组 | 路径 | 页面 |
|------|------|------|
| 认证 | `/login` `/register` `/change-password` | 登录/注册/改密 |
| Home | `/home` | 经营概览与待办 |
| Content | `/content/pages` `/content/articles` `/content/products` `/content/media` | 内容管理 |
| AI | `/ai/assistant` `/ai/tasks` `/ai/approvals` | AI 助手 / 任务 / 审批 |
| Business | `/business/customers` `/business/forms` `/business/leads` | 客户 / 表单 / 线索 |
| Automation | `/automation/workflows` `/automation/integrations` | 工作流 / 集成 |
| Team | `/team/users` `/team/roles` | 成员 / 角色权限 |
| Settings | `/settings` | 个人资料 / 偏好 / 安全 |
| 移动/公开 | `/m` `/f.$formId` | 移动审批快捷页 / 公开表单页 |

---

## 目录结构（web/src）

```
api/         client(API 核心) · auth · articles · cms/{index,store,http,seed,types}
components/  cms/(表格/表单/页头/状态/权限预览) · common/(StatCard/StatusBadge/DragVerify/RichTextEditor/RoleMultiSelect)
             charts/(Sparkline/AreaChart/DonutChart) · layout/(Header/MainLayout/Sidebar/PageContainer) · navigation/TabBar
config/      nav(IA) · permissions(码表+角色矩阵)
hooks/       usePermission · useTabOpener
i18n/        zh-CN / en 双语
lib/         business · error · utils · modelInfer · permissions
store/       auth · config · language · permission · tabs (Zustand)
routes/      __root + 各页面（TanStack file-based）
```

---

## 权限 UX（按钮级）

- `usePermission()` 提供 `has(perm)`；导航（`config/nav.ts` `filterNavByPerm`）与按钮据权限显隐。
- 角色预览切换器（`/team/roles`）可实时切换 owner/editor/viewer 观察菜单与按钮差异（演示/验收用）。
- 后端返回 403 时，`client.ts` 弹权限 toast；导航与按钮只是 UX 镜像，**真正校验在后端**。

---

## 关键交互

- **AI 助手**（`/ai/assistant`）：输入「发布…」→ 调 `content.articles.publish`（Editor 无码自动转审批）；其余指令 → `content.articles.draft` 生成草稿。`?q=` 可携带指令从全局搜索/Home 卡片直达并执行。
- **审批中心**（`/ai/approvals`）：待办列表，`approvalsApi.decide()` 直连 `POST /api/approvals/{id}/decide`（服务端幂等 + 强制裁决权）。
- **工作流**（`/automation/workflows`）：列表 + 启停开关 + 新建/编辑弹窗（选择触发事件）；`real` 模式展示最近运行（取自审计流水）。
- **移动工作台**（`/m`）：手机浏览器 / PWA 打开的移动端**多标签 App**，不套主布局。**底部标签栏**：① 待办审批（待我审批 15s 轮询 + 角标 / 已办，详情抽屉内批准·驳回，仅 Owner 可裁决）② 概览（并行拉各资源 `total` + 待审数，卡片点进内容）③ 内容（articles/pages/products/media/customers/leads/forms 分段切换 + 列表 + 详情 + 通用表单**全功能 CRUD**，`viewer` 角色隐藏写按钮）④ 我的（`/api/user/me` 资料 + 改密 + 退出）。直连真实后端，无 mock 分支。

---

## 国际化 / 主题

- i18n：中文（默认）+ 英文，`react-i18next`，`store/language.ts` 持久化。
- 主题/布局/字号：`store/config.ts` 支持浅色/深色/跟随系统、多套布局模板、字号调节，纯客户端生效。

---

## 测试

- **Vitest**：18 个测试文件，覆盖 `api`/`components/cms`/`components/common`/`config`/`lib`/`store`（auth/config/language/permission/tabs）。当前实测 **115/115 通过**。
- **Playwright E2E**：`web/e2e/` 含 `auth` / `approvals` / `mobile-approvals` / `team-audit` 等规格。

## PWA / 移动端「添加到主屏幕」

移动工作台 `/m` 已支持以**独立 App**形式安装到手机主屏幕（满足 PWA 安装条件）：

- `public/manifest.webmanifest`：`name`/`short_name`=`审批台`、`start_url=/m`、`display=standalone`、`theme_color=#4F46E5`、3 个图标（192/512 + maskable-512）。
- `public/sw.js`（Service Worker）：安装时预缓存应用壳 + 图标；**`/api/*` 绝不缓存**（审批数据实时）；Vite 开发模块（`/@`、`/src/`、`.vite`、`?t=`）跳过缓存以不破坏 HMR；页面导航 network-first 离线回退 `/m` 壳；静态资源 stale-while-revalidate。
- `index.html`：`<link rel="manifest">`、`<meta name="theme-color">`、iOS `apple-touch-icon` / `apple-mobile-web-app-capable` 等链路。
- `src/main.tsx`：`window.load` 后 `navigator.serviceWorker.register('/sw.js')`（注册失败静默忽略，不影响主流程）。
- 图标由 `public/icons/icon-{192,512}.png` 与 `icon-maskable-512.png` 提供（indigo 圆角底 + 白色对勾）。

> 真机「添加到主屏幕」需 **HTTPS**（localhost 下 Chrome/Edge 可直接试用；局域网 IP 访问需部署到 HTTPS 域名）。

---

## 已知前端缺口（详见 COMPLETION.md）

- `/settings` 的「个人资料保存」与「修改密码」目前为 **前端 dev mock**（未调用后端 `/api/me/password`；但独立 `/change-password` 路由与后端接口已存在）。
- 工作流 **可视化节点编辑器**（`@xyflow/react`/`cytoscape` 已引入）尚未实现，当前为列表 + 弹窗 MVP。
- 集成页（OAuth 凭证填写/连接）与后端的 `oauth/start` 联动 UI 待补全。
