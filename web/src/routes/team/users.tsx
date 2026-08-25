import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, Modal, toast, Input, Label, type UseOverlayStateReturn } from '@heroui/react'
import { createFileRoute } from '@tanstack/react-router'

import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'
import { fmtRelative } from '../../components/cms/format'
import { StatusBadge, toneFromStatus } from '../../components/common/StatusBadge'
import { CMS_MODE } from '../../api/cms'
import { api } from '../../api/client'

/** 成员（mock 为静态演示；real 来自 /api/team/users） */
interface TeamMember {
  id: string
  name: string
  username?: string
  role: 'owner' | 'editor' | 'viewer'
  status: string | number
  lastActiveAt?: string
  createdAt?: string
}

const MOCK_MEMBERS: TeamMember[] = [
  { id: 'u1', name: '林小茶（我）', role: 'owner', status: 'Active', lastActiveAt: new Date().toISOString() },
  { id: 'u2', name: '陈师傅', role: 'editor', status: 'Active', lastActiveAt: new Date(Date.now() - 3600_000).toISOString() },
  { id: 'u3', name: '周周', role: 'editor', status: 'Active', lastActiveAt: new Date(Date.now() - 86400_000).toISOString() },
  { id: 'u4', name: '王会计', role: 'viewer', status: 'Inactive' },
]

const ROLE_META: Record<TeamMember['role'], { label: string; cls: string }> = {
  owner: { label: 'Owner · 老板', cls: 'bg-indigo-50 text-indigo-600' },
  editor: { label: 'Editor · 可以编辑内容', cls: 'bg-blue-50 text-blue-600' },
  viewer: { label: 'Viewer · 只能查看', cls: 'bg-gray-100 text-gray-500' },
}

const isActive = (s: string | number) => s === 'Active' || s === 1

function TeamUsersPage() {
  const qc = useQueryClient()
  const [inviteOpen, setInviteOpenRaw] = useState(false)
  const inviteModal: UseOverlayStateReturn = {
    isOpen: inviteOpen,
    setOpen: (v) => setInviteOpenRaw(v),
    open: () => setInviteOpenRaw(true),
    close: () => setInviteOpenRaw(false),
    toggle: () => setInviteOpenRaw((v) => !v),
  }
  const [form, setForm] = useState({ username: '', nickname: '', role: 'editor' as TeamMember['role'] })

  const isReal = CMS_MODE === 'real'
  const { data: members = [] } = useQuery({
    queryKey: ['team-members', CMS_MODE],
    queryFn: async (): Promise<TeamMember[]> => {
      if (!isReal) return MOCK_MEMBERS
      const rows = await api<{ id: string; username: string; nickname: string; role: TeamMember['role']; status: number; createdAt: string }[]>('/api/team/users')
      return rows.map((r) => ({
        id: r.id, name: r.nickname || r.username, username: r.username,
        role: r.role, status: r.status, createdAt: r.createdAt,
      }))
    },
  })

  const invite = useMutation({
    mutationFn: (input: typeof form) =>
      api('/api/team/users', { method: 'POST', body: JSON.stringify(input) }),
    onSuccess: () => {
      toast.success('已邀请，初始密码 demo1234')
      inviteModal.close()
      setForm({ username: '', nickname: '', role: 'editor' })
      void qc.invalidateQueries({ queryKey: ['team-members'] })
    },
  })

  const patch = useMutation({
    mutationFn: ({ id, ...updates }: { id: string; role?: string; status?: number }) =>
      api(`/api/team/users/${id}`, { method: 'PUT', body: JSON.stringify(updates) }),
    onSuccess: () => {
      toast.success('已更新')
      void qc.invalidateQueries({ queryKey: ['team-members'] })
    },
  })

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader
        title="成员"
        desc="谁在用这个系统，各自能做什么。"
        actions={
          <Auth perm={P.teamUsersInvite}>
            <Button variant="primary" size="sm" onPress={inviteModal.open}>
              + 邀请成员
            </Button>
          </Auth>
        }
      />

      <div className="rounded-xl border border-os-border bg-white shadow-sm divide-y divide-os-border-light overflow-hidden">
        {members.map((m) => (
          <div key={m.id} className="flex items-center gap-4 px-4 py-3.5 hover:bg-os-bg-hover transition-colors flex-wrap">
            {/* 头像 */}
            <span className="w-9 h-9 rounded-full bg-gradient-to-br from-indigo-400 to-violet-500 text-white text-sm font-medium flex items-center justify-center flex-shrink-0">
              {m.name[0]}
            </span>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-os-text-primary">{m.name}</p>
              <p className="text-xs text-os-text-muted mt-0.5">
                {m.username ? `@${m.username} · ` : ''}
                {m.lastActiveAt ? `最近活跃 ${fmtRelative(m.lastActiveAt)}` : m.createdAt ? `加入于 ${fmtRelative(m.createdAt)}` : '从未登录'}
              </p>
            </div>

            {/* 角色变更（real 且有邀请权时开放） */}
            {isReal ? (
              <Auth perm={P.teamUsersInvite}>
                <select
                  value={m.role}
                  aria-label={`变更 ${m.name} 的角色`}
                  onChange={(e) => patch.mutate({ id: m.id, role: e.target.value })}
                  className="h-7 px-1.5 text-xs rounded-md border border-os-border bg-white outline-none focus:border-[#6366f1]"
                >
                  {(Object.keys(ROLE_META) as TeamMember['role'][]).map((k) => (
                    <option key={k} value={k}>{ROLE_META[k].label}</option>
                  ))}
                </select>
              </Auth>
            ) : (
              <span className={`text-xs px-2 py-0.5 rounded-md font-medium ${ROLE_META[m.role].cls}`}>
                {ROLE_META[m.role].label}
              </span>
            )}

            <StatusBadge tone={toneFromStatus(isActive(m.status) ? 'Active' : 'Inactive')} dot>
              {isActive(m.status) ? '正常' : '停用'}
            </StatusBadge>

            {/* 停用 / 启用 */}
            {isReal && (
              <Auth perm={P.teamUsersInvite} mode="disable">
                <Button
                  variant="ghost" size="sm"
                  className={isActive(m.status) ? 'text-os-danger-text hover:bg-os-danger-bg' : ''}
                  onPress={() => patch.mutate({ id: m.id, status: isActive(m.status) ? 0 : 1 })}
                >
                  {isActive(m.status) ? '停用' : '启用'}
                </Button>
              </Auth>
            )}
          </div>
        ))}
      </div>

      <p className="text-xs text-os-text-muted mt-4 px-1">
        权限模型：Owner 可管理团队与审批；Editor 可编辑内容但不能直接发布；Viewer 只读。
        AI 同样遵守这套权限 —— 它是执行者，不是超级管理员。
      </p>

      {/* 邀请弹窗（real 模式） */}
      {isReal && (
        <Modal state={inviteModal}>
          <Modal.Backdrop />
          <Modal.Container>
            <Modal.Dialog>
              <Modal.Header>
                <h3 className="text-base font-semibold">邀请成员</h3>
              </Modal.Header>
              <Modal.Body>
                <div className="flex flex-col gap-3">
                  <div>
                    <Label htmlFor="inv-username">用户名（登录用）</Label>
                    <Input
                      id="inv-username" value={form.username}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, username: e.target.value })}
                      placeholder="如 xiaozhou"
                    />
                  </div>
                  <div>
                    <Label htmlFor="inv-nickname">显示名</Label>
                    <Input
                      id="inv-nickname" value={form.nickname}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, nickname: e.target.value })}
                      placeholder="如 收银小周"
                    />
                  </div>
                  <div>
                    <Label htmlFor="inv-role">角色</Label>
                    <select
                      id="inv-role" value={form.role}
                      onChange={(e) => setForm({ ...form, role: e.target.value as TeamMember['role'] })}
                      className="w-full h-9 px-2 text-sm rounded-lg border border-os-border bg-white outline-none focus:border-[#6366f1]"
                    >
                      {(Object.keys(ROLE_META) as TeamMember['role'][]).map((k) => (
                        <option key={k} value={k}>{ROLE_META[k].label}</option>
                      ))}
                    </select>
                  </div>
                  <p className="text-xs text-os-text-muted">初始密码 demo1234；对方首次登录时会被要求修改。</p>
                </div>
              </Modal.Body>
              <Modal.Footer>
                <Button variant="ghost" size="sm" onPress={inviteModal.close}>取消</Button>
                <Button
                  variant="primary" size="sm"
                  isDisabled={!form.username.trim() || !form.nickname.trim() || invite.isPending}
                  onPress={() => invite.mutate(form)}
                >
                  {invite.isPending ? '创建中…' : '创建账号'}
                </Button>
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal>
      )}
    </div>
  )
}

export const Route = createFileRoute('/team/users')({
  component: TeamUsersPage,
})
