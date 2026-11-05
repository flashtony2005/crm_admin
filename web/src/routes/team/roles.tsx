import { useState, useMemo, useEffect, Fragment } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { Button, Label, toast } from '@heroui/react'

import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { usePermission } from '../../hooks/usePermission'
import { usePermissionStore } from '../../store/permission'
import {
  PERMISSION_GROUPS,
  type RoleDef,
  type PermString,
  type PermCode,
} from '../../config/permissions'
import { teamUsersApi, type TeamMember } from '../../api/cms'

function ShieldIcon() {
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
    </svg>
  )
}

function KeyIcon() {
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="7.5" cy="15.5" r="5.5" />
      <path d="m21 2-9.6 9.6 M15.5 7.5l3 3 M18.5 4.5l3 3" />
    </svg>
  )
}

function UserGroupIcon() {
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2 M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z M23 21v-2a4 4 0 0 0-3-3.87 M16 3.13a4 4 0 0 1 0 7.75" />
    </svg>
  )
}

function EditIcon() {
  return (
    <svg width={14} height={14} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z" />
    </svg>
  )
}

function TrashIcon() {
  return (
    <svg width={14} height={14} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="3 6 5 6 21 6" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
    </svg>
  )
}

function PlusIcon() {
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <line x1="12" y1="5" x2="12" y2="19" />
      <line x1="5" y1="12" x2="19" y2="12" />
    </svg>
  )
}

function CheckIcon() {
  return (
    <svg width={14} height={14} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="20 6 9 17 4 12" />
    </svg>
  )
}

function SearchIcon() {
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="11" cy="11" r="8" />
      <line x1="21" y1="21" x2="16.65" y2="16.65" />
    </svg>
  )
}

function TeamRolesPage() {
  const { role: activePreviewRole, setRole: setPreviewRole } = usePermission()
  const {
    roles,
    rolePerms,
    updateRole,
    addRole,
    deleteRole,
    updateRolePerms,
    togglePermForRole,
    resetToDefault,
  } = usePermissionStore()

  // 状态与弹窗
  const [activeTab, setActiveTab] = useState<'cards' | 'matrix'>('cards')
  const [teamMembers, setTeamMembers] = useState<TeamMember[]>([])
  const [editingRole, setEditingRole] = useState<RoleDef | null>(null)
  const [authorizingRole, setAuthorizingRole] = useState<RoleDef | null>(null)
  const [assigningRole, setAssigningRole] = useState<RoleDef | null>(null)
  const [createOpen, setCreateOpen] = useState(false)

  // 授权抽屉内的草稿权限集与检索词
  const [authDraftPerms, setAuthDraftPerms] = useState<PermString[]>([])
  const [authSearch, setAuthSearch] = useState('')
  const [matrixSearch, setMatrixSearch] = useState('')

  // 新建角色表单状态
  const [newRoleForm, setNewRoleForm] = useState({
    key: '',
    name: '',
    desc: '',
    aiBoundary: '',
    copyFrom: 'editor',
  })

  // 加载系统团队成员以统计角色人数
  const reloadMembers = () => {
    void teamUsersApi.list().then((list) => {
      setTeamMembers(list)
    })
  }

  useEffect(() => {
    reloadMembers()
  }, [])

  // 统计每个角色的成员数
  const roleMemberCounts = useMemo(() => {
    const counts: Record<string, number> = {}
    for (const m of teamMembers) {
      counts[m.role] = (counts[m.role] || 0) + 1
    }
    return counts
  }, [teamMembers])

  // 打开授权弹窗并初始化权限草稿
  const handleOpenAuthorize = (r: RoleDef) => {
    setAuthorizingRole(r)
    const current = rolePerms[r.key] || (r.key === 'owner' ? ['*'] : [])
    setAuthDraftPerms([...current])
    setAuthSearch('')
  }

  // 保存角色授权
  const handleSaveAuthorize = () => {
    if (!authorizingRole) return
    updateRolePerms(authorizingRole.key, authDraftPerms)
    toast.success(`已保存角色「${authorizingRole.name}」的权限配置`)
    setAuthorizingRole(null)
  }

  // 切换单项草稿权限
  const toggleDraftPerm = (code: PermCode) => {
    setAuthDraftPerms((prev) => {
      if (prev.includes('*')) {
        // 如果原本是全量权限，解包为具体权限再减去该项
        const allCodes = PERMISSION_GROUPS.flatMap((g) => g.permissions.map((p) => p.code))
        return allCodes.filter((c) => c !== code)
      }
      if (prev.includes(code)) {
        return prev.filter((p) => p !== code)
      }
      return [...prev, code]
    })
  }

  // 批量选择某组全部权限
  const handleSelectGroup = (groupCodes: PermCode[]) => {
    setAuthDraftPerms((prev) => {
      const set = new Set(prev)
      groupCodes.forEach((c) => set.add(c))
      return Array.from(set)
    })
  }

  // 批量清空某组权限
  const handleDeselectGroup = (groupCodes: PermCode[]) => {
    setAuthDraftPerms((prev) => {
      if (prev.includes('*')) {
        const allCodes = PERMISSION_GROUPS.flatMap((g) => g.permissions.map((p) => p.code))
        const remain = allCodes.filter((c) => !groupCodes.includes(c))
        return remain
      }
      return prev.filter((p) => !groupCodes.includes(p as PermCode))
    })
  }

  // 全选所有权限
  const handleSelectAllPerms = () => {
    const allCodes = PERMISSION_GROUPS.flatMap((g) => g.permissions.map((p) => p.code))
    setAuthDraftPerms(allCodes)
  }

  // 清空所有权限
  const handleClearAllPerms = () => {
    setAuthDraftPerms([])
  }

  // 新建角色提交
  const handleCreateRoleSubmit = () => {
    const key = newRoleForm.key.trim().toLowerCase()
    const name = newRoleForm.name.trim()
    if (!key || !name) {
      toast.danger('角色标识与名称不能为空')
      return
    }
    if (roles.some((r) => r.key === key)) {
      toast.danger('该角色标识已存在，请更换')
      return
    }

    let initialPerms: PermString[] = []
    if (newRoleForm.copyFrom && rolePerms[newRoleForm.copyFrom]) {
      initialPerms = [...rolePerms[newRoleForm.copyFrom]]
    }

    addRole(
      {
        key,
        name,
        desc: newRoleForm.desc.trim() || '自定义业务角色',
        aiBoundary: newRoleForm.aiBoundary.trim() || '按所授权限严格限制 AI 操作范围',
        isSystem: false,
        cls: 'border-purple-200 bg-gradient-to-br from-purple-50/40 to-white',
      },
      initialPerms,
    )

    toast.success(`角色「${name}」已创建，并已初始化权限`)
    setCreateOpen(false)
    setNewRoleForm({ key: '', name: '', desc: '', aiBoundary: '', copyFrom: 'editor' })
  }

  // 删除自定义角色
  const handleDeleteCustomRole = (r: RoleDef) => {
    if (r.isSystem) {
      toast.danger('系统内置角色不可删除')
      return
    }
    if ((roleMemberCounts[r.key] || 0) > 0) {
      toast.danger(`该角色下仍有 ${roleMemberCounts[r.key]} 名成员，请先调整成员归属后再删除`)
      return
    }
    if (confirm(`确定要删除自定义角色「${r.name}」吗？`)) {
      deleteRole(r.key)
      toast.success(`已删除角色「${r.name}」`)
    }
  }

  // 分配成员归属
  const handleToggleMemberRole = async (member: TeamMember, targetRoleKey: string) => {
    const isCurrentlyInRole = member.role === targetRoleKey
    const nextRole = isCurrentlyInRole ? 'viewer' : targetRoleKey
    try {
      await teamUsersApi.update(member.id, { role: nextRole })
      toast.success(`已将「${member.name}」分配为 ${nextRole}`)
      reloadMembers()
    } catch (e) {
      toast.danger((e as Error).message || '分配失败')
    }
  }

  return (
    <div className="p-2 md:p-4 flex flex-col gap-6 max-w-7xl mx-auto">
      {/* 顶部标题与主要操作 */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <CmsPageHeader
          title="角色与权限管理"
          desc="配置团队系统角色、细粒度功能权限，以及每个角色下的 AI 智能体执行边界。"
        />
        <div className="flex items-center gap-2 shrink-0">
          <Button
            variant="ghost"
            size="sm"
            onPress={() => {
              if (confirm('确定要恢复系统初始默认的角色与权限设置吗？')) {
                resetToDefault()
                toast.success('已恢复默认权限矩阵')
              }
            }}
            className="text-os-text-secondary text-xs"
          >
            重置默认预设
          </Button>
          <Button
            variant="primary"
            size="sm"
            onPress={() => setCreateOpen(true)}
            className="flex items-center gap-1.5"
          >
            <PlusIcon /> 新建角色
          </Button>
        </div>
      </div>

      {/* 权限预览切换器条 */}
      <div className="rounded-xl border border-indigo-200 bg-gradient-to-r from-indigo-50/70 via-indigo-50/30 to-white p-3.5 flex items-center justify-between gap-4 flex-wrap shadow-xs">
        <div className="flex items-center gap-3">
          <span className="flex items-center justify-center w-7 h-7 rounded-lg bg-indigo-600 text-white shadow-xs">
            <KeyIcon />
          </span>
          <div>
            <div className="text-sm font-semibold text-os-text-primary flex items-center gap-2">
              <span>当前模拟生效角色：</span>
              <select
                value={activePreviewRole}
                onChange={(e) => setPreviewRole(e.target.value)}
                aria-label="当前模拟生效角色"
                className="h-8 px-2 text-sm font-medium rounded-lg border border-indigo-300 bg-white text-indigo-900 outline-none focus:ring-2 focus:ring-indigo-400"
              >
                {roles.map((r) => (
                  <option key={r.key} value={r.key}>
                    {r.name} ({r.key})
                  </option>
                ))}
              </select>
            </div>
            <p className="text-xs text-os-text-muted mt-0.5">
              切换后本系统的所有按钮、导航及操作权限将立即响应更新，无需重新登录即可验收。
            </p>
          </div>
        </div>

        {/* 视图切换 */}
        <div className="flex items-center rounded-lg border border-os-border bg-white p-0.5">
          <button
            type="button"
            onClick={() => setActiveTab('cards')}
            className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
              activeTab === 'cards'
                ? 'bg-indigo-600 text-white shadow-xs'
                : 'text-os-text-secondary hover:text-os-text-primary'
            }`}
          >
            角色卡片与授权
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('matrix')}
            className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
              activeTab === 'matrix'
                ? 'bg-indigo-600 text-white shadow-xs'
                : 'text-os-text-secondary hover:text-os-text-primary'
            }`}
          >
            全局权限矩阵
          </button>
        </div>
      </div>

      {/* ── 视图 1：角色卡片与授权 ── */}
      {activeTab === 'cards' && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {roles.map((r) => {
            const memberCount = roleMemberCounts[r.key] || 0
            const perms = rolePerms[r.key] || []
            const isOwner = perms.includes('*') || r.key === 'owner'
            const permCountText = isOwner ? '拥有全部特权 (*)' : `已授予 ${perms.length} 项功能权限`

            return (
              <div
                key={r.key}
                className={`rounded-2xl border p-5 flex flex-col justify-between transition-all hover:shadow-md ${
                  r.cls || 'border-os-border bg-white'
                }`}
              >
                <div>
                  <div className="flex items-start justify-between gap-2 mb-2">
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="text-base font-bold text-os-text-primary">{r.name}</h3>
                        {r.isSystem ? (
                          <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium bg-gray-100 text-gray-600 border border-gray-200">
                            系统内置
                          </span>
                        ) : (
                          <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium bg-purple-100 text-purple-700 border border-purple-200">
                            自定义
                          </span>
                        )}
                      </div>
                      <span className="text-xs font-mono text-os-text-muted">标识: {r.key}</span>
                    </div>

                    <button
                      type="button"
                      onClick={() => setAssigningRole(r)}
                      className="inline-flex items-center gap-1 px-2.5 py-1 text-xs font-medium rounded-full bg-os-bg-base border border-os-border hover:bg-os-bg-hover transition-colors"
                      title="点击为该角色分配成员"
                    >
                      <UserGroupIcon />
                      <span>{memberCount} 人</span>
                    </button>
                  </div>

                  <p className="text-xs text-os-text-secondary leading-relaxed mt-2 min-h-[36px]">
                    {r.desc}
                  </p>

                  <div className="mt-3 p-2.5 rounded-xl bg-white/80 border border-indigo-100 text-xs text-indigo-900 leading-normal">
                    <span className="font-semibold text-indigo-700 flex items-center gap-1 mb-1">
                      <ShieldIcon /> AI 智能体边界：
                    </span>
                    <span className="text-indigo-800/90">{r.aiBoundary}</span>
                  </div>

                  <div className="mt-3 flex items-center gap-2 text-xs text-os-text-muted">
                    <KeyIcon />
                    <span>{permCountText}</span>
                  </div>
                </div>

                {/* 底部操作栏 */}
                <div className="mt-5 pt-3.5 border-t border-os-border-light flex items-center justify-between gap-2">
                  <div className="flex items-center gap-1.5">
                    <Button
                      variant="primary"
                      size="sm"
                      onPress={() => handleOpenAuthorize(r)}
                      className="text-xs flex items-center gap-1"
                    >
                      <KeyIcon /> 授权权限
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onPress={() => setEditingRole(r)}
                      className="text-xs flex items-center gap-1"
                    >
                      <EditIcon /> 编辑
                    </Button>
                  </div>

                  {!r.isSystem && (
                    <button
                      type="button"
                      onClick={() => handleDeleteCustomRole(r)}
                      className="p-1.5 text-os-text-muted hover:text-red-600 rounded-lg hover:bg-red-50 transition-colors"
                      title="删除自定义角色"
                    >
                      <TrashIcon />
                    </button>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      )}

      {/* ── 视图 2：全局交互式权限矩阵 ── */}
      {activeTab === 'matrix' && (
        <div className="rounded-2xl border border-os-border bg-white shadow-xs overflow-hidden">
          <div className="p-4 border-b border-os-border bg-os-bg-base flex flex-col sm:flex-row items-center justify-between gap-3">
            <div className="relative w-full sm:w-72">
              <span className="absolute left-3 top-1/2 -translate-y-1/2 text-os-text-muted">
                <SearchIcon />
              </span>
              <input
                type="text"
                placeholder="搜索功能或权限码..."
                value={matrixSearch}
                onChange={(e) => setMatrixSearch(e.target.value)}
                className="w-full h-9 pl-9 pr-3 text-xs rounded-lg border border-os-border bg-white outline-none focus:border-indigo-500"
              />
            </div>
            <p className="text-xs text-os-text-muted">
              提示：点击复选框可直接开通或收回角色的对应能力，修改自动实时存储。
            </p>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse text-xs">
              <thead>
                <tr className="bg-gray-50 border-b border-os-border text-os-text-secondary font-medium uppercase tracking-wider">
                  <th className="py-3 px-4 w-72">权限能力名称 / 编码</th>
                  <th className="py-3 px-4">说明</th>
                  {roles.map((r) => (
                    <th key={r.key} className="py-3 px-4 text-center min-w-[90px]">
                      <div className="font-bold text-os-text-primary text-xs">{r.name}</div>
                      <div className="text-[10px] text-os-text-muted font-mono">{r.key}</div>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody className="divide-y divide-os-border-light">
                {PERMISSION_GROUPS.map((group) => {
                  const filtered = group.permissions.filter((p) => {
                    if (!matrixSearch) return true
                    const q = matrixSearch.toLowerCase()
                    return p.name.toLowerCase().includes(q) || p.code.toLowerCase().includes(q) || p.desc.toLowerCase().includes(q)
                  })

                  if (filtered.length === 0) return null

                  return (
                    <Fragment key={group.id}>
                      {/* 分组标题行 */}
                      <tr className="bg-indigo-50/50">
                        <td colSpan={2 + roles.length} className="py-2.5 px-4 font-bold text-indigo-900 text-xs">
                          {group.title} ({filtered.length})
                        </td>
                      </tr>

                      {/* 组内权限项 */}
                      {filtered.map((perm) => (
                        <tr key={perm.code} className="hover:bg-os-bg-hover transition-colors">
                          <td className="py-3 px-4">
                            <div className="font-semibold text-os-text-primary text-xs">{perm.name}</div>
                            <div className="text-[11px] font-mono text-os-text-muted mt-0.5">{perm.code}</div>
                          </td>
                          <td className="py-3 px-4 text-os-text-secondary text-xs">{perm.desc}</td>

                          {roles.map((r) => {
                            const current = rolePerms[r.key] || []
                            const isGranted = current.includes('*') || current.includes(perm.code)
                            const isOwner = r.key === 'owner'

                            return (
                              <td key={r.key} className="py-3 px-4 text-center">
                                <label className="inline-flex items-center justify-center cursor-pointer p-1">
                                  <input
                                    type="checkbox"
                                    checked={isGranted}
                                    disabled={isOwner}
                                    onChange={() => {
                                      togglePermForRole(r.key, perm.code)
                                      toast.success(`已更新 ${r.name} 的「${perm.name}」授权`)
                                    }}
                                    className="w-4 h-4 text-indigo-600 rounded border-gray-300 focus:ring-indigo-500 cursor-pointer disabled:opacity-70 disabled:cursor-not-allowed"
                                  />
                                </label>
                              </td>
                            )
                          })}
                        </tr>
                      ))}
                    </Fragment>
                  )
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* ── 弹窗 1：角色独立授权弹窗 (可以授权) ── */}
      {authorizingRole && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-6 bg-black/40 backdrop-blur-xs">
          <div className="w-full max-w-3xl max-h-[90vh] bg-white rounded-2xl shadow-2xl border border-os-border flex flex-col overflow-hidden animate-in fade-in duration-150">
            {/* 头部 */}
            <div className="p-4 sm:p-5 border-b border-os-border bg-os-bg-base flex items-center justify-between">
              <div>
                <h3 className="text-base font-bold text-os-text-primary flex items-center gap-2">
                  <KeyIcon /> 授权权限：{authorizingRole.name}
                </h3>
                <p className="text-xs text-os-text-muted mt-0.5">
                  为该角色分配系统各个功能模块的操作权限。勾选即可开放，未勾选将被安全拦截。
                </p>
              </div>
              <button
                type="button"
                onClick={() => setAuthorizingRole(null)}
                className="w-8 h-8 rounded-lg hover:bg-gray-200 flex items-center justify-center text-gray-500 text-lg"
              >
                ✕
              </button>
            </div>

            {/* 检索与快捷工具栏 */}
            <div className="p-3.5 border-b border-os-border-light bg-white flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3">
              <div className="relative flex-1">
                <span className="absolute left-3 top-1/2 -translate-y-1/2 text-os-text-muted">
                  <SearchIcon />
                </span>
                <input
                  type="text"
                  placeholder="搜索功能名称或权限码..."
                  value={authSearch}
                  onChange={(e) => setAuthSearch(e.target.value)}
                  className="w-full h-8 pl-9 pr-3 text-xs rounded-lg border border-os-border outline-none focus:border-indigo-500"
                />
              </div>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={handleSelectAllPerms}
                  className="px-2.5 py-1 text-xs font-medium rounded-md border border-os-border bg-os-bg-base hover:bg-os-bg-hover text-os-text-primary"
                >
                  全选所有
                </button>
                <button
                  type="button"
                  onClick={handleClearAllPerms}
                  className="px-2.5 py-1 text-xs font-medium rounded-md border border-os-border bg-os-bg-base hover:bg-os-bg-hover text-os-text-secondary"
                >
                  清空
                </button>
                <span className="text-xs text-indigo-700 font-semibold px-2 py-0.5 rounded-full bg-indigo-50">
                  已选 {authDraftPerms.includes('*') ? '全部 (*)' : `${authDraftPerms.length} 项`}
                </span>
              </div>
            </div>

            {/* 权限树列表 */}
            <div className="flex-1 overflow-y-auto p-4 sm:p-5 space-y-6">
              {PERMISSION_GROUPS.map((group) => {
                const groupFiltered = group.permissions.filter((p) => {
                  if (!authSearch) return true
                  const q = authSearch.toLowerCase()
                  return p.name.toLowerCase().includes(q) || p.code.toLowerCase().includes(q) || p.desc.toLowerCase().includes(q)
                })

                if (groupFiltered.length === 0) return null

                const groupCodes = group.permissions.map((p) => p.code)

                return (
                  <div key={group.id} className="rounded-xl border border-os-border p-4 bg-gray-50/50">
                    <div className="flex items-center justify-between pb-3 mb-3 border-b border-os-border-light">
                      <div className="flex items-center gap-2">
                        <span className="font-bold text-sm text-os-text-primary">{group.title}</span>
                        <span className="text-xs text-os-text-muted">({groupFiltered.length} 项)</span>
                      </div>
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          onClick={() => handleSelectGroup(groupCodes)}
                          className="text-xs text-indigo-600 hover:text-indigo-800"
                        >
                          全选本组
                        </button>
                        <span className="text-gray-300">|</span>
                        <button
                          type="button"
                          onClick={() => handleDeselectGroup(groupCodes)}
                          className="text-xs text-gray-500 hover:text-gray-700"
                        >
                          清空本组
                        </button>
                      </div>
                    </div>

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                      {groupFiltered.map((p) => {
                        const checked = authDraftPerms.includes('*') || authDraftPerms.includes(p.code)
                        return (
                          <label
                            key={p.code}
                            className={`flex items-start gap-2.5 p-2.5 rounded-lg border cursor-pointer transition-colors ${
                              checked
                                ? 'bg-indigo-50/80 border-indigo-300 text-indigo-950'
                                : 'bg-white border-os-border-light hover:bg-gray-50 text-os-text-primary'
                            }`}
                          >
                            <input
                              type="checkbox"
                              checked={checked}
                              onChange={() => toggleDraftPerm(p.code)}
                              className="mt-0.5 w-4 h-4 rounded text-indigo-600 border-gray-300 focus:ring-indigo-500"
                            />
                            <div className="flex-1 min-w-0">
                              <div className="text-xs font-semibold leading-tight flex items-center justify-between">
                                <span>{p.name}</span>
                              </div>
                              <p className="text-[11px] text-os-text-muted mt-0.5 leading-snug line-clamp-1">{p.desc}</p>
                              <code className="text-[10px] text-gray-400 font-mono">{p.code}</code>
                            </div>
                          </label>
                        )
                      })}
                    </div>
                  </div>
                )
              })}
            </div>

            {/* 底部保存 */}
            <div className="p-4 border-t border-os-border bg-os-bg-base flex items-center justify-between">
              <span className="text-xs text-os-text-muted">
                授权结果将即刻同步至系统权限中心并本地持久化存储。
              </span>
              <div className="flex items-center gap-2">
                <Button variant="ghost" size="sm" onPress={() => setAuthorizingRole(null)}>
                  取消
                </Button>
                <Button variant="primary" size="sm" onPress={handleSaveAuthorize} className="flex items-center gap-1">
                  <CheckIcon /> 保存授权
                </Button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* ── 弹窗 2：编辑角色信息 (可以编辑) ── */}
      {editingRole && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs">
          <div className="w-full max-w-lg bg-white rounded-2xl shadow-xl border border-os-border overflow-hidden">
            <div className="p-5 border-b border-os-border bg-os-bg-base flex items-center justify-between">
              <h3 className="text-base font-bold text-os-text-primary flex items-center gap-2">
                <EditIcon /> 编辑角色信息
              </h3>
              <button
                type="button"
                onClick={() => setEditingRole(null)}
                className="w-8 h-8 rounded-lg hover:bg-gray-200 flex items-center justify-center text-gray-500"
              >
                ✕
              </button>
            </div>

            <div className="p-5 space-y-4">
              <div>
                <Label htmlFor="edit-role-key" className="text-xs font-medium text-os-text-secondary">
                  角色标识 (Key)
                </Label>
                <input
                  id="edit-role-key"
                  type="text"
                  disabled
                  value={editingRole.key}
                  className="w-full h-9 px-3 mt-1 text-xs rounded-lg border border-os-border bg-gray-100 font-mono text-gray-500 cursor-not-allowed"
                />
              </div>

              <div>
                <Label htmlFor="edit-role-name" className="text-xs font-medium text-os-text-secondary">
                  角色名称
                </Label>
                <input
                  id="edit-role-name"
                  type="text"
                  value={editingRole.name}
                  onChange={(e) => setEditingRole({ ...editingRole, name: e.target.value })}
                  className="w-full h-9 px-3 mt-1 text-xs rounded-lg border border-os-border outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <Label htmlFor="edit-role-desc" className="text-xs font-medium text-os-text-secondary">
                  角色职责描述
                </Label>
                <textarea
                  id="edit-role-desc"
                  rows={2}
                  value={editingRole.desc}
                  onChange={(e) => setEditingRole({ ...editingRole, desc: e.target.value })}
                  className="w-full p-2.5 mt-1 text-xs rounded-lg border border-os-border outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <Label htmlFor="edit-role-ai" className="text-xs font-medium text-indigo-700">
                  ✦ AI 智能体执行边界（限制 AI 的高风险写入权限）
                </Label>
                <textarea
                  id="edit-role-ai"
                  rows={2}
                  value={editingRole.aiBoundary}
                  onChange={(e) => setEditingRole({ ...editingRole, aiBoundary: e.target.value })}
                  className="w-full p-2.5 mt-1 text-xs rounded-lg border border-indigo-200 bg-indigo-50/30 text-indigo-950 outline-none focus:border-indigo-500"
                />
              </div>
            </div>

            <div className="p-4 border-t border-os-border bg-os-bg-base flex items-center justify-end gap-2">
              <Button variant="ghost" size="sm" onPress={() => setEditingRole(null)}>
                取消
              </Button>
              <Button
                variant="primary"
                size="sm"
                onPress={() => {
                  updateRole(editingRole.key, {
                    name: editingRole.name,
                    desc: editingRole.desc,
                    aiBoundary: editingRole.aiBoundary,
                  })
                  toast.success(`角色「${editingRole.name}」已更新`)
                  setEditingRole(null)
                }}
              >
                保存修改
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* ── 弹窗 3：新建角色 ── */}
      {createOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs">
          <div className="w-full max-w-lg bg-white rounded-2xl shadow-xl border border-os-border overflow-hidden">
            <div className="p-5 border-b border-os-border bg-os-bg-base flex items-center justify-between">
              <h3 className="text-base font-bold text-os-text-primary flex items-center gap-2">
                <PlusIcon /> 新建自定义角色
              </h3>
              <button
                type="button"
                onClick={() => setCreateOpen(false)}
                className="w-8 h-8 rounded-lg hover:bg-gray-200 flex items-center justify-center text-gray-500"
              >
                ✕
              </button>
            </div>

            <div className="p-5 space-y-4">
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <Label htmlFor="new-role-key" className="text-xs font-medium text-os-text-secondary">
                    角色英文标识 (Key) *
                  </Label>
                  <input
                    id="new-role-key"
                    type="text"
                    placeholder="如 finance / support"
                    value={newRoleForm.key}
                    onChange={(e) => setNewRoleForm({ ...newRoleForm, key: e.target.value })}
                    className="w-full h-9 px-3 mt-1 text-xs rounded-lg border border-os-border outline-none font-mono focus:border-indigo-500"
                  />
                </div>
                <div>
                  <Label htmlFor="new-role-name" className="text-xs font-medium text-os-text-secondary">
                    角色中文名 *
                  </Label>
                  <input
                    id="new-role-name"
                    type="text"
                    placeholder="如 财务专员 / 客服"
                    value={newRoleForm.name}
                    onChange={(e) => setNewRoleForm({ ...newRoleForm, name: e.target.value })}
                    className="w-full h-9 px-3 mt-1 text-xs rounded-lg border border-os-border outline-none focus:border-indigo-500"
                  />
                </div>
              </div>

              <div>
                <Label htmlFor="new-role-copy" className="text-xs font-medium text-os-text-secondary">
                  初始权限继承自已有角色
                </Label>
                <select
                  id="new-role-copy"
                  value={newRoleForm.copyFrom}
                  onChange={(e) => setNewRoleForm({ ...newRoleForm, copyFrom: e.target.value })}
                  className="w-full h-9 px-3 mt-1 text-xs rounded-lg border border-os-border bg-white outline-none focus:border-indigo-500"
                >
                  <option value="">空白权限（无任何权限）</option>
                  {roles.map((r) => (
                    <option key={r.key} value={r.key}>
                      复制「{r.name}」的当前权限集
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <Label htmlFor="new-role-desc" className="text-xs font-medium text-os-text-secondary">
                  职责描述
                </Label>
                <textarea
                  id="new-role-desc"
                  rows={2}
                  placeholder="该角色负责的具体工作范围..."
                  value={newRoleForm.desc}
                  onChange={(e) => setNewRoleForm({ ...newRoleForm, desc: e.target.value })}
                  className="w-full p-2.5 mt-1 text-xs rounded-lg border border-os-border outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <Label htmlFor="new-role-ai" className="text-xs font-medium text-indigo-700">
                  ✦ AI 智能体执行边界
                </Label>
                <textarea
                  id="new-role-ai"
                  rows={2}
                  placeholder="限制 AI 在此角色身份下可以做和不可以做的事情..."
                  value={newRoleForm.aiBoundary}
                  onChange={(e) => setNewRoleForm({ ...newRoleForm, aiBoundary: e.target.value })}
                  className="w-full p-2.5 mt-1 text-xs rounded-lg border border-indigo-200 bg-indigo-50/30 text-indigo-950 outline-none focus:border-indigo-500"
                />
              </div>
            </div>

            <div className="p-4 border-t border-os-border bg-os-bg-base flex items-center justify-end gap-2">
              <Button variant="ghost" size="sm" onPress={() => setCreateOpen(false)}>
                取消
              </Button>
              <Button variant="primary" size="sm" onPress={handleCreateRoleSubmit}>
                立即创建
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* ── 弹窗 4：分配角色成员 ── */}
      {assigningRole && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs">
          <div className="w-full max-w-md bg-white rounded-2xl shadow-xl border border-os-border overflow-hidden">
            <div className="p-5 border-b border-os-border bg-os-bg-base flex items-center justify-between">
              <div>
                <h3 className="text-base font-bold text-os-text-primary flex items-center gap-2">
                  <UserGroupIcon /> 分配成员：{assigningRole.name}
                </h3>
                <p className="text-xs text-os-text-muted mt-0.5">
                  勾选成员将直接把该成员角色变更为「{assigningRole.name}」
                </p>
              </div>
              <button
                type="button"
                onClick={() => setAssigningRole(null)}
                className="w-8 h-8 rounded-lg hover:bg-gray-200 flex items-center justify-center text-gray-500"
              >
                ✕
              </button>
            </div>

            <div className="p-4 max-h-[60vh] overflow-y-auto space-y-2">
              {teamMembers.length === 0 ? (
                <p className="text-xs text-os-text-muted py-4 text-center">暂无成员记录</p>
              ) : (
                teamMembers.map((m) => {
                  const isInRole = m.role === assigningRole.key
                  return (
                    <div
                      key={m.id}
                      className={`flex items-center justify-between p-3 rounded-xl border transition-colors ${
                        isInRole
                          ? 'border-indigo-300 bg-indigo-50/70 text-indigo-950'
                          : 'border-os-border bg-white hover:bg-gray-50'
                      }`}
                    >
                      <div className="flex items-center gap-3">
                        <span className="w-8 h-8 rounded-full bg-gradient-to-br from-indigo-500 to-purple-600 text-white text-xs font-bold flex items-center justify-center">
                          {m.name[0]}
                        </span>
                        <div>
                          <p className="text-xs font-bold text-os-text-primary">{m.name}</p>
                          <p className="text-[10px] text-os-text-muted">
                            {m.username ? `@${m.username} · ` : ''}当前角色: {m.role}
                          </p>
                        </div>
                      </div>

                      <Button
                        variant={isInRole ? 'primary' : 'outline'}
                        size="sm"
                        onPress={() => handleToggleMemberRole(m, assigningRole.key)}
                        className="text-xs"
                      >
                        {isInRole ? '已加入 (移除)' : '加入角色'}
                      </Button>
                    </div>
                  )
                })
              )}
            </div>

            <div className="p-4 border-t border-os-border bg-os-bg-base flex justify-end">
              <Button variant="primary" size="sm" onPress={() => setAssigningRole(null)}>
                完成
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export const Route = createFileRoute('/team/roles')({
  component: TeamRolesPage,
})
