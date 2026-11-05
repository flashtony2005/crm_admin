import { useState, useMemo } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, Label, toast } from '@heroui/react'
import { createFileRoute } from '@tanstack/react-router'

import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'
import { fmtRelative } from '../../components/cms/format'
import { StatusBadge, toneFromStatus } from '../../components/common/StatusBadge'
import { teamUsersApi, type TeamMember } from '../../api/cms'
import { usePermissionStore } from '../../store/permission'

function PlusIcon() {
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <line x1="12" y1="5" x2="12" y2="19" />
      <line x1="5" y1="12" x2="19" y2="12" />
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

function TrashIcon() {
  return (
    <svg width={14} height={14} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="3 6 5 6 21 6" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
    </svg>
  )
}

const isActive = (s: string | number | undefined) => s === 'Active' || s === 1 || s === 'active'

function TeamUsersPage() {
  const qc = useQueryClient()
  const { roles } = usePermissionStore()

  const [inviteOpen, setInviteOpen] = useState(false)
  const [search, setSearch] = useState('')
  const [roleFilter, setRoleFilter] = useState('')
  const [form, setForm] = useState({ username: '', name: '', role: 'editor' })


  // 查询团队成员列表
  const { data: members = [], isLoading } = useQuery({
    queryKey: ['team-members'],
    queryFn: async (): Promise<TeamMember[]> => {
      return await teamUsersApi.list()
    },
  })

  // 邀请 / 创建新成员
  const invite = useMutation({
    mutationFn: async (input: typeof form) => {
      return await teamUsersApi.create({
        name: input.name.trim(),
        username: input.username.trim(),
        role: input.role,
        status: 'Active',
        lastActiveAt: new Date().toISOString(),
      })
    },
    onSuccess: () => {
      toast.success('已添加成员，初始密码 demo1234')
      setInviteOpen(false)
      setForm({ username: '', name: '', role: 'editor' })
      void qc.invalidateQueries({ queryKey: ['team-members'] })
    },
    onError: (err: Error) => {
      toast.danger(err.message || '邀请成员失败')
    },
  })

  // 更新成员属性（角色、启用/停用）
  const patch = useMutation({
    mutationFn: async ({ id, ...updates }: { id: string; role?: string; status?: string | number }) => {
      return await teamUsersApi.update(id, updates)
    },
    onSuccess: () => {
      toast.success('成员信息已更新')
      void qc.invalidateQueries({ queryKey: ['team-members'] })
    },
    onError: (err: Error) => {
      toast.danger(err.message || '更新成员失败')
    },
  })

  // 删除成员
  const remove = useMutation({
    mutationFn: async (id: string) => {
      return await teamUsersApi.remove(id)
    },
    onSuccess: () => {
      toast.success('已移除成员')
      void qc.invalidateQueries({ queryKey: ['team-members'] })
    },
    onError: (err: Error) => {
      toast.danger(err.message || '移除成员失败')
    },
  })

  // 搜索和角色过滤
  const filteredMembers = useMemo(() => {
    return members.filter((m) => {
      if (roleFilter && m.role !== roleFilter) return false
      if (!search.trim()) return true
      const q = search.toLowerCase()
      return (
        m.name.toLowerCase().includes(q) ||
        (m.username && m.username.toLowerCase().includes(q))
      )
    })
  }, [members, search, roleFilter])

  return (
    <div className="p-2 md:p-4 max-w-7xl mx-auto flex flex-col gap-6">
      <CmsPageHeader
        title="团队成员管理"
        desc="配置使用管理后台的团队成员、各自担当的角色以及账号启停状态。"
        actions={
          <Auth perm={P.teamUsersInvite}>
            <Button
              variant="primary"
              size="sm"
              onPress={() => setInviteOpen(true)}
              className="flex items-center gap-1.5"
            >
              <PlusIcon /> 邀请新成员
            </Button>
          </Auth>
        }
      />

      {/* 搜索与过滤工具栏 */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3 bg-white p-3.5 rounded-xl border border-os-border shadow-xs">
        <div className="relative flex-1 max-w-md">
          <span className="absolute left-3 top-1/2 -translate-y-1/2 text-os-text-muted">
            <SearchIcon />
          </span>
          <input
            type="text"
            placeholder="按姓名或用户名搜索成员..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full h-9 pl-9 pr-3 text-xs rounded-lg border border-os-border outline-none focus:border-indigo-500"
          />
        </div>

        <div className="flex items-center gap-2">
          <label className="text-xs text-os-text-secondary whitespace-nowrap">角色筛选：</label>
          <select
            value={roleFilter}
            onChange={(e) => setRoleFilter(e.target.value)}
            aria-label="角色筛选"
            className="h-9 px-2 text-xs rounded-lg border border-os-border bg-white outline-none focus:border-indigo-500"
          >
            <option value="">全部角色 ({members.length})</option>
            {roles.map((r) => (
              <option key={r.key} value={r.key}>
                {r.name}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* 成员列表卡片 */}
      <div className="rounded-2xl border border-os-border bg-white shadow-xs divide-y divide-os-border-light overflow-hidden">
        {isLoading ? (
          <div className="py-12 text-center text-xs text-os-text-muted">加载成员列表中...</div>
        ) : filteredMembers.length === 0 ? (
          <div className="py-12 text-center text-xs text-os-text-muted">未找到匹配的成员</div>
        ) : (
          filteredMembers.map((m) => {
            const active = isActive(m.status)

            return (
              <div
                key={m.id}
                className="flex items-center gap-4 px-5 py-4 hover:bg-os-bg-hover transition-colors flex-wrap justify-between"
              >
                {/* 成员基本信息 */}
                <div className="flex items-center gap-3.5 min-w-[200px]">
                  <span className="w-10 h-10 rounded-full bg-gradient-to-br from-indigo-500 to-purple-600 text-white text-sm font-bold flex items-center justify-center shrink-0 shadow-xs">
                    {m.name ? m.name[0] : 'U'}
                  </span>
                  <div>
                    <div className="flex items-center gap-2">
                      <p className="text-sm font-bold text-os-text-primary">{m.name}</p>
                      {m.role === 'owner' && (
                        <span className="text-[10px] px-1.5 py-0.2 rounded font-semibold bg-indigo-50 text-indigo-700 border border-indigo-200">
                          超级管理员
                        </span>
                      )}
                    </div>
                    <p className="text-xs text-os-text-muted mt-0.5">
                      {m.username ? `@${m.username} · ` : ''}
                      {m.lastActiveAt
                        ? `最近活跃 ${fmtRelative(m.lastActiveAt)}`
                        : m.createdAt
                        ? `加入于 ${fmtRelative(m.createdAt)}`
                        : '从未登录'}
                    </p>
                  </div>
                </div>

                {/* 角色与状态控制操作 */}
                <div className="flex items-center gap-3 flex-wrap">
                  {/* 角色变更下拉框 */}
                  <div className="flex items-center gap-1.5">
                    <span className="text-xs text-os-text-secondary hidden sm:inline">分配角色:</span>
                    <select
                      value={m.role}
                      aria-label={`变更 ${m.name} 的角色`}
                      onChange={(e) => patch.mutate({ id: m.id, role: e.target.value })}
                      className="h-8 px-2 text-xs rounded-lg border border-os-border bg-white text-os-text-primary outline-none focus:border-indigo-500 font-medium"
                    >
                      {roles.map((r) => (
                        <option key={r.key} value={r.key}>
                          {r.name}
                        </option>
                      ))}
                    </select>
                  </div>

                  {/* 账号状态指示 */}
                  <StatusBadge tone={toneFromStatus(active ? 'Active' : 'Inactive')} dot>
                    {active ? '正常运行' : '已停用'}
                  </StatusBadge>

                  {/* 停用 / 启用按钮 */}
                  <Button
                    variant="ghost"
                    size="sm"
                    className={`text-xs ${active ? 'text-amber-600 hover:bg-amber-50' : 'text-green-600 hover:bg-green-50'}`}
                    onPress={() => patch.mutate({ id: m.id, status: active ? 'Inactive' : 'Active' })}
                  >
                    {active ? '停用账号' : '恢复启用'}
                  </Button>

                  {/* 删除成员按钮 */}
                  {m.role !== 'owner' && (
                    <button
                      type="button"
                      onClick={() => {
                        if (confirm(`确定要移除成员「${m.name}」吗？移除后对方将无法登录后台。`)) {
                          remove.mutate(m.id)
                        }
                      }}
                      className="p-1.5 text-os-text-muted hover:text-red-600 rounded-lg hover:bg-red-50 transition-colors"
                      title="移除成员"
                    >
                      <TrashIcon />
                    </button>
                  )}
                </div>
              </div>
            )
          })
        )}
      </div>

      <div className="p-4 rounded-xl border border-indigo-100 bg-indigo-50/40 text-xs text-indigo-900 leading-relaxed">
        <p className="font-semibold text-indigo-950 mb-1">💡 团队协同与安全边界：</p>
        <p>
          所有账号执行的高风险操作（如文章发布、配置保存、成员变更）均会自动记录到系统审计日志中。
          如需定制角色的具体能力（如只能看报表不能改数据），请前往<strong>「角色与权限」</strong>页面进行精细化授权。
        </p>
      </div>

      {/* 邀请成员弹窗 */}
      {inviteOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-xs">
          <div className="w-full max-w-md bg-white rounded-2xl shadow-xl border border-os-border overflow-hidden">
            <div className="p-5 border-b border-os-border bg-os-bg-base flex items-center justify-between">
              <h3 className="text-base font-bold text-os-text-primary flex items-center gap-2">
                <PlusIcon /> 邀请 / 添加成员
              </h3>
              <button
                type="button"
                onClick={() => setInviteOpen(false)}
                className="w-8 h-8 rounded-lg hover:bg-gray-200 flex items-center justify-center text-gray-500"
              >
                ✕
              </button>
            </div>

            <div className="p-5 space-y-4">
              <div>
                <Label htmlFor="inv-name" className="text-xs font-medium text-os-text-secondary">
                  成员显示名 *
                </Label>
                <input
                  id="inv-name"
                  type="text"
                  placeholder="如 收银小周 / 李运营"
                  value={form.name}
                  onChange={(e) => setForm({ ...form, name: e.target.value })}
                  className="w-full h-9 px-3 mt-1 text-xs rounded-lg border border-os-border outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <Label htmlFor="inv-username" className="text-xs font-medium text-os-text-secondary">
                  登录用户名 *
                </Label>
                <input
                  id="inv-username"
                  type="text"
                  placeholder="如 xiaozhou (用于系统登录)"
                  value={form.username}
                  onChange={(e) => setForm({ ...form, username: e.target.value })}
                  className="w-full h-9 px-3 mt-1 text-xs rounded-lg border border-os-border outline-none font-mono focus:border-indigo-500"
                />
              </div>

              <div>
                <Label htmlFor="inv-role" className="text-xs font-medium text-os-text-secondary">
                  所属角色 *
                </Label>
                <select
                  id="inv-role"
                  value={form.role}
                  onChange={(e) => setForm({ ...form, role: e.target.value })}
                  className="w-full h-9 px-3 mt-1 text-xs rounded-lg border border-os-border bg-white outline-none focus:border-indigo-500"
                >
                  {roles.map((r) => (
                    <option key={r.key} value={r.key}>
                      {r.name} ({r.key})
                    </option>
                  ))}
                </select>
                <p className="text-[11px] text-os-text-muted mt-1">
                  该成员登录后将享有该角色被配置的所有功能权限。
                </p>
              </div>

              <div className="p-3 rounded-lg bg-gray-50 border border-gray-200 text-xs text-os-text-muted">
                新账号初始密码为 <code className="font-mono text-indigo-600 font-bold">demo1234</code>，对方首次登录后可在个人中心随时修改。
              </div>
            </div>

            <div className="p-4 border-t border-os-border bg-os-bg-base flex items-center justify-end gap-2">
              <Button variant="ghost" size="sm" onPress={() => setInviteOpen(false)}>
                取消
              </Button>
              <Button
                variant="primary"
                size="sm"
                isDisabled={!form.username.trim() || !form.name.trim() || invite.isPending}
                onPress={() => invite.mutate(form)}
              >
                {invite.isPending ? '创建中...' : '确认创建'}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export const Route = createFileRoute('/team/users')({
  component: TeamUsersPage,
})
