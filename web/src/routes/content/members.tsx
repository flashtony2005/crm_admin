import { createFileRoute } from '@tanstack/react-router'
import { useCallback, useEffect, useState } from 'react'
import { Button, Input, Switch, toast } from '@heroui/react'
import { inviteCodesApi, membersApi, type InviteCode, type Member } from '../../api/cms'
import { request } from '../../api/client'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

/** 邀请制注册开关：读公开 site 配置，写走 /api/admin/site（site.settings.update） */
function InviteGateToggle() {
  const [on, setOn] = useState<boolean | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    void (async () => {
      try {
        const r = await request<{ data: { memberInviteRequired?: string } }>('/api/public/site')
        setOn(r?.data?.memberInviteRequired === 'on')
      } catch {
        setOn(false)
      }
    })()
  }, [])

  const toggle = useCallback(async (v: boolean) => {
    setBusy(true)
    try {
      await request('/api/admin/site', {
        method: 'PUT',
        body: JSON.stringify({ member_invite_required: v ? 'on' : 'off' }),
        headers: { 'Content-Type': 'application/json' },
      })
      setOn(v)
      toast.success(v ? '已开启邀请制：注册必须凭邀请码' : '已关闭邀请制：开放注册')
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : '保存失败')
    } finally {
      setBusy(false)
    }
  }, [])

  return (
    <div className="flex items-center gap-3 rounded-xl border border-default-200 px-4 py-3">
      <div className="min-w-0">
        <p className="text-sm font-medium">邀请制注册</p>
        <p className="text-xs text-os-text-muted">开启后，公开站注册必须填写有效邀请码（Owner 可改）</p>
      </div>
      <div className="ml-auto flex items-center gap-2">
        {on === null ? (
          <span className="text-xs text-os-text-muted">加载中…</span>
        ) : (
          <Auth perm="site.settings.update" mode="disable">
            <Switch isSelected={on} isDisabled={busy} onChange={(e: any) => void toggle(e.target.checked)} />
          </Auth>
        )}
      </div>
    </div>
  )
}

const EMPTY_DRAFT = { code: '', quota: '5', expiresAt: '' }

function InviteCodesPanel() {
  const t = useCmsCollection(inviteCodesApi, ['cms-invite-codes'], { searchFields: ['code'] })
  const [draft, setDraft] = useState(EMPTY_DRAFT)
  const [creating, setCreating] = useState(false)

  const create = useCallback(async () => {
    const code = draft.code.trim()
    if (!code) {
      toast.danger('请填写邀请码')
      return
    }
    setCreating(true)
    try {
      await t.create.mutateAsync({
        code,
        ownerMemberId: '',
        quota: Number(draft.quota) || 1,
        used: 0,
        expiresAt: draft.expiresAt.trim(),
        enabled: true,
      })
      setDraft(EMPTY_DRAFT)
      toast.success(`邀请码 ${code} 已创建`)
    } catch (e) {
      toast.danger(e instanceof Error ? e.message : '创建失败（码可能已存在）')
    } finally {
      setCreating(false)
    }
  }, [draft, t])

  const toggleEnabled = useCallback(
    async (row: InviteCode) => {
      await t.update.mutateAsync({ id: row.id, patch: { enabled: !row.enabled } })
    },
    [t],
  )

  const columns: CmsColumn<InviteCode>[] = [
    { id: 'code', header: '邀请码', render: (r) => <span className="font-mono font-medium">{r.code}</span> },
    {
      id: 'usage',
      header: '核销 / 额度',
      render: (r) => {
        const full = Number(r.used) >= Number(r.quota)
        return (
          <span className={`text-xs tabular-nums ${full ? 'text-os-danger-text' : 'text-os-text-secondary'}`}>
            {r.used} / {r.quota}{full ? '（已用完）' : ''}
          </span>
        )
      },
    },
    { id: 'expiresAt', header: '过期时间', render: (r) => <span className="text-xs">{r.expiresAt ? fmtDate(r.expiresAt) : '永不过期'}</span> },
    {
      id: 'enabled',
      header: '状态',
      render: (r) => (
        <span className={`px-2 py-0.5 rounded-full text-xs ${r.enabled ? 'bg-emerald-50 text-emerald-600' : 'bg-gray-100 text-gray-500'}`}>
          {r.enabled ? '启用' : '停用'}
        </span>
      ),
    },
  ]

  return (
    <div className="mt-6 space-y-3">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-sm font-medium">邀请码</p>
          <p className="text-xs text-os-text-muted">注册核销自动 +1；ownerMemberId 可留空（运营码）</p>
        </div>
      </div>

      <Auth perm={P.contentMembersCreate} mode="disable">
        <div className="flex flex-wrap items-center gap-2 rounded-xl border border-default-200 p-3">
          <Input
            value={draft.code}
            onChange={(e: any) => setDraft({ ...draft, code: e.target.value })}
            placeholder="邀请码，如 WELCOME-2026"
            className="w-44"
          />
          <Input
            value={draft.quota}
            onChange={(e: any) => setDraft({ ...draft, quota: e.target.value.replace(/\D/g, '') })}
            placeholder="额度"
            className="w-24"
          />
          <Input
            value={draft.expiresAt}
            onChange={(e: any) => setDraft({ ...draft, expiresAt: e.target.value })}
            placeholder="过期时间（空=永久），如 2026-12-31"
            className="w-56"
          />
          <Button variant="primary" size="sm" isDisabled={creating} onPress={() => void create()}>
            创建邀请码
          </Button>
        </div>
      </Auth>

      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="🎟️"
        emptyTitle="还没有邀请码"
        emptyHint="创建一个邀请码，分享给要邀请的会员"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Auth perm={P.contentMembersUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => void toggleEnabled(row)}>
                {row.enabled ? '停用' : '启用'}
              </Button>
            </Auth>
            <Auth perm={P.contentMembersDelete}>
              <Button
                variant="ghost"
                size="sm"
                className="text-os-danger-text hover:bg-os-danger-bg"
                onPress={() => void t.remove.mutateAsync(row.id)}
              >
                删除
              </Button>
            </Auth>
          </div>
        )}
      />
    </div>
  )
}

function MembersPage() {
  const [search, setSearch] = useState('')
  const t = useCmsCollection(membersApi, ['cms-members'], { searchFields: ['email', 'name', 'plan'], serverPaged: true })

  const columns: CmsColumn<Member>[] = [
    { id: 'email', header: '邮箱', render: (r) => <span className="font-medium">{r.email}</span> },
    { id: 'name', header: '昵称', render: (r) => <span>{r.name || '—'}</span> },
    {
      id: 'plan', header: '套餐', render: (r) =>
        <span className={`px-2 py-0.5 rounded-full text-xs ${r.plan === 'free' ? 'bg-gray-100 text-gray-600' : 'bg-purple-100 text-purple-700'}`}>{r.plan}</span>,
    },
    {
      id: 'invited', header: '来源', render: (r) =>
        r.invitedBy ? (
          <span className="px-2 py-0.5 rounded-full text-xs bg-rose-50 text-rose-600">邀请加入</span>
        ) : (
          <span className="text-xs text-os-text-muted">自然注册</span>
        ),
    },
    { id: 'status', header: '状态', render: (r) => <span>{r.status === 1 ? '正常' : '停用'}</span> },
    { id: 'createdAt', header: '注册时间', render: (r) => <time className="text-xs text-os-text-muted">{fmtDate(r.createdAt)}</time> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="会员" desc="注册会员列表与套餐分布。会员可在公开站注册、登录并访问会员专属内容。" />
      <InviteGateToggle />
      <CmsToolbar searchPlaceholder="搜索邮箱 / 昵称 / 套餐…" searchValue={search} onSearchChange={setSearch}>
        <Auth perm={P.contentMembersCreate}>
          <Button variant="primary" size="sm" onPress={() => alert('会员通过公开站 /membership 自助注册')}>说明</Button>
        </Auth>
      </CmsToolbar>
      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="👥"
        emptyTitle="还没有会员"
        emptyHint="会员在公开站的「会员」页面自助注册"
      />
      <InviteCodesPanel />
    </div>
  )
}

export const Route = createFileRoute('/content/members')({ component: MembersPage })
