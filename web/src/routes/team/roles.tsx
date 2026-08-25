import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { usePermission } from '../../hooks/usePermission'
import { ROLE_PERMS, type RoleKey } from '../../config/permissions'
import { createFileRoute } from '@tanstack/react-router'

/** 角色权限矩阵（Phase 1 演示数据；Phase 3 接入 Capability/Policy 后可配置） */
interface RoleDef {
  key: string
  name: string
  desc: string
  /** AI 权限边界说明（PRODUCT_VISION §7：AI 是执行者，不是超级管理员） */
  aiBoundary: string
  memberCount: number
  cls: string
}

const ROLES: RoleDef[] = [
  {
    key: 'owner',
    name: 'Owner（老板）',
    desc: '全部管理权限：团队、审批、设置、账单。',
    aiBoundary: 'AI 可以请求发布，高风险操作仍需本人批准。',
    memberCount: 1,
    cls: 'border-indigo-200 bg-gradient-to-br from-indigo-50/60 to-white',
  },
  {
    key: 'editor',
    name: 'Editor（店员）',
    desc: '创建和编辑内容、查看客户与线索。',
    aiBoundary: 'AI 可以修改草稿；不能直接发布，发布必须经 Owner 审批。',
    memberCount: 2,
    cls: '',
  },
  {
    key: 'viewer',
    name: 'Viewer（观察者）',
    desc: '只读访问：看数据和报表，不能改动。',
    aiBoundary: 'AI 仅可执行查询类操作（搜索、摘要）。',
    memberCount: 1,
    cls: '',
  },
]

const PERMISSIONS: { label: string; owner: boolean; editor: boolean; viewer: boolean }[] = [
  { label: '内容编辑（Pages / Articles / Products）', owner: true, editor: true, viewer: false },
  { label: '直接发布内容', owner: true, editor: false, viewer: false },
  { label: '客户与线索管理', owner: true, editor: true, viewer: false },
  { label: '审批 AI 操作', owner: true, editor: false, viewer: false },
  { label: '团队成员与角色管理', owner: true, editor: false, viewer: false },
  { label: '集成与系统设置', owner: true, editor: false, viewer: false },
]

function Mark({ ok }: { ok: boolean }) {
  return ok ? (
    <span className="text-green-600 font-semibold">✓</span>
  ) : (
    <span className="text-gray-300">—</span>
  )
}

const ROLE_LABEL: Record<RoleKey, string> = {
  owner: 'Owner（老板）',
  editor: 'Editor（店员）',
  viewer: 'Viewer（观察者）',
}

/** 权限预览切换器：切换后立即作用于全站按钮与导航（演示/验收用） */
function RolePreviewSwitcher() {
  const { role, setRole } = usePermission()
  const permCount = ROLE_PERMS[role].length === 1 && ROLE_PERMS[role][0] === '*'
    ? '全量权限'
    : `${ROLE_PERMS[role].length} 项权限`
  return (
    <div className="rounded-xl border border-dashed border-indigo-300 bg-indigo-50/50 p-3.5 flex items-center gap-3 flex-wrap">
      <span className="text-sm text-os-text-secondary">🔍 权限预览：</span>
      <select
        value={role}
        onChange={(e) => setRole(e.target.value as RoleKey)}
        aria-label="预览角色"
        className="h-8 px-2 text-sm rounded-lg border border-os-border bg-white outline-none focus:border-[#6366f1]"
      >
        {(Object.keys(ROLE_LABEL) as RoleKey[]).map((k) => (
          <option key={k} value={k}>{ROLE_LABEL[k]}</option>
        ))}
      </select>
      <span className="text-xs text-os-text-muted">当前 {permCount}</span>
    </div>
  )
}

function TeamRolesPage() {
  return (
    <div className="p-1 md:p-2 flex flex-col gap-5">
      <CmsPageHeader title="角色与权限" desc="每个角色能做什么、AI 在这个角色下能做什么。" />
      <RolePreviewSwitcher />

      {/* 角色卡片 */}
      <div className="grid md:grid-cols-3 gap-3">
        {ROLES.map((r) => (
          <div key={r.key} className={`rounded-xl border border-os-border p-4 shadow-sm ${r.cls}`}>
            <div className="flex items-center justify-between mb-2">
              <h3 className="text-sm font-semibold text-os-text-primary">{r.name}</h3>
              <span className="text-xs text-os-text-muted">{r.memberCount} 人</span>
            </div>
            <p className="text-xs text-os-text-muted">{r.desc}</p>
            <p className="text-xs mt-3 pt-3 border-t border-os-border-light text-[#6366F1]">
              ✦ {r.aiBoundary}
            </p>
          </div>
        ))}
      </div>

      {/* 权限矩阵 */}
      <div className="rounded-xl border border-os-border bg-white shadow-sm overflow-x-auto">
        <table className="w-full text-sm [&_td]:px-4 [&_td]:py-3 [&_th]:px-4 [&_th]:py-3 [&_th]:text-left">
          <thead className="bg-os-bg-base text-xs uppercase tracking-wider text-os-text-secondary">
            <tr>
              <th>能力</th>
              <th className="w-20">Owner</th>
              <th className="w-20">Editor</th>
              <th className="w-20">Viewer</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-os-border-light">
            {PERMISSIONS.map((p) => (
              <tr key={p.label} className="hover:bg-os-bg-hover transition-colors">
                <td className="text-os-text-primary">{p.label}</td>
                <td><Mark ok={p.owner} /></td>
                <td><Mark ok={p.editor} /></td>
                <td><Mark ok={p.viewer} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <p className="text-xs text-os-text-muted px-1">
        底层链路：Permission → Capability → Policy → Approval → Action → Audit（对用户不可见，只在这里如实呈现）。
      </p>
    </div>
  )
}

export const Route = createFileRoute('/team/roles')({
  component: TeamRolesPage,
})
