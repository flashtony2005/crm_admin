# 工作流「新增/编辑」按钮不显示的修复与部署说明

## 问题现象
- Workflows 页（`/automation/workflows`）的「+ 新建流程 / 编辑 / 编辑流程图 / 启停开关」全部不显示；
- 左侧导航栏的 Workflows / Integrations 入口也可能消失；
- 使用 **owner（老板）** 账号正常，**editor（店员）/ viewer（观察者）** 账号异常。

## 根因
后端 `server/src/perm.rs` 的角色权限矩阵中，`Editor` 与 `Viewer` 只有
`automation.workflows.view`，缺少 `automation.workflows.toggle`。

前端 `web/src/routes/automation/workflows.tsx` 用
`canToggle = has('automation.workflows.toggle')` 控制上述按钮显隐；
`web/src/config/nav.ts` 的侧栏入口 `perm` 同样是 `toggle`。
→ editor/viewer 登录时按钮与入口全部隐藏，后端对 workflows 的 create/update 也
强制校验 `toggle`（`resources.rs`），所以既看不到、也调不通。

## 已完成的代码修复（直接使用当前源码重新编译即可）
| 文件 | 改动 |
|---|---|
| `server/src/perm.rs` | Editor 矩阵新增 `automation.workflows.toggle`、`automation.integrations.toggle`（Viewer 仍只读） |
| `web/src/config/permissions.ts` | 前端镜像同步（editor 增加两个 toggle） |
| `web/src/routes/automation/workflows.tsx` | HeroUI v3 非法 `variant="flat"` → `variant="ghost"` |
| `web/src/routes/automation/WorkflowEditor.tsx` | 非法 `variant="flat"/"light"`、`color="primary"` → 合法取值 |
| `server/src/db.rs` | SQLx 连接超时 5s → 30s（本机 cms.db 被锁阻塞 5s 的健壮性修复） |

## 部署步骤（在系统终端执行，不要用 WorkBuddy 的 Bash）

### 1. 重新编译后端
```powershell
cd F:\project\admin\server
cargo build
```
> 若报 `.cargo-build-lock` 拒绝访问：这是本机安全软件按文件名拦截的已知问题，
> 请将 `F:\project\admin\server` 加入杀毒/Defender 排除项，或改用管理员终端重试。

### 2. 重启后端（先停旧进程，再起新二进制）
停旧进程（按端口找 PID）：
```powershell
$pid = (Get-NetTCPConnection -LocalPort 8088 -State Listen).OwningProcess
Stop-Process -Id $pid -Force
```
启动新后端（推荐显式指定数据库路径，避开 cms.db 被锁的 5s 阻塞问题）：
```powershell
cd F:\project\admin\server
$env:DATABASE_URL = "sqlite:F:/project/admin/server/cms_live.db?mode=rwc"
.\target\debug\cms-server.exe
```
> `cms_live.db` 是 `cms.db` 的副本（含全部种子数据），本机 cms.db 被某监控进程
> 持续锁定 5 秒导致 SQLx 连接超时，用副本可立即连接。若您本机 cms.db 无此问题，
> 可直接用 `sqlite:F:/project/admin/server/cms.db?mode=rwc`。

### 3. 验证修复
```powershell
# 用 editor 登录拿 token
$token = (Invoke-RestMethod -Method Post -Uri http://localhost:8088/api/auth/login `
  -ContentType 'application/json' -Body '{"username":"editor","password":"demo1234"}').data.token
# 查看权限集
(Invoke-RestMethod -Uri http://localhost:8088/api/user/me `
  -Headers @{Authorization = "Bearer $token"}).data.permissions
```
输出中应包含 `automation.workflows.toggle`。前端刷新页面后，
editor 登录即可看到「+ 新建流程 / 编辑 / 编辑流程图」按钮。

## 临时应急（不想编译时）
将 editor 账号角色临时改为 owner（立即获得全部权限，含 toggle）：
```powershell
sqlite3 F:\project\admin\server\cms_live.db "UPDATE users SET role='owner' WHERE username='editor';"
```
> ⚠️ 仅应急用：editor 会获得 owner 全部权限（含审批裁决）。正式修复请按上文重编部署。

## 服务当前状态（截至本次调试结束）
- 前端：http://localhost:5188（Vite dev server，运行中）
- 后端：http://localhost:8088（旧二进制 `target3/debug/cms-server.exe` + cms_live.db，运行中）
- 代码修复已就绪，待按本说明重编部署后生效。
