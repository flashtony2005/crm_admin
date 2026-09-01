import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Button } from '@heroui/react'

import { CHANNEL_TYPES, domainApi, type DvChannel } from '../../api/domain'
import type { FormFieldDef } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

function channelCrud(): import('../../api/cms/store').CrudService<DvChannel> {
  return {
    list: () => domainApi.channels.list(),
    get: async (id) => {
      try {
        return await domainApi.channels.get(id)
      } catch {
        return undefined
      }
    },
    create: async (input) => {
      const id = await domainApi.channels.create(input as unknown as Record<string, unknown>)
      return { ...(input as unknown as DvChannel), id, createdAt: '', updatedAt: '' }
    },
    update: (id, patch) => domainApi.channels.update(id, patch),
    remove: (id) => domainApi.channels.remove(id),
  }
}

const FORM_FIELDS: FormFieldDef[] = [
  { key: 'name', label: '渠道名称', type: 'text', required: true, placeholder: '如：Website / X / 微信公众号' },
  {
    key: 'type',
    label: '渠道类型',
    type: 'select',
    required: true,
    options: CHANNEL_TYPES.map((v) => ({ value: v, label: v })),
    defaultValue: 'website',
  },
  { key: 'url', label: 'URL', type: 'text', placeholder: '如 https://x.com/yourname' },
  { key: 'provider', label: 'Provider（可选）', type: 'text', placeholder: '如 zapier / make' },
  {
    key: 'status',
    label: '状态',
    type: 'select',
    options: ['active', 'inactive'].map((v) => ({ value: v, label: v })),
    defaultValue: 'active',
  },
  { key: 'externalId', label: '外部 ID（可选）', type: 'text', placeholder: '渠道侧的身份标识' },
]

function ChannelsPage() {
  const qc = useQueryClient()
  const [editing, setEditing] = useState<DvChannel | null>(null)
  const [modalOpen, setModalOpen] = useState(false)

  const t = useCmsCollection(channelCrud(), ['domain-channels'], {
    searchFields: ['name', 'type', 'url', 'provider'],
  })

  const refresh = () => qc.invalidateQueries({ queryKey: ['domain-channels'] })

  const handleSubmit = async (values: Record<string, string | number>) => {
    const patch: Record<string, unknown> = {
      name: String(values.name),
      type: String(values.type),
      url: String(values.url ?? '').trim() || undefined,
      provider: String(values.provider ?? '').trim() || undefined,
      status: String(values.status ?? 'active'),
      externalId: String(values.externalId ?? '').trim() || undefined,
    }
    if (editing) await domainApi.channels.update(editing.id, patch)
    else await domainApi.channels.create(patch)
    refresh()
  }

  const handleDelete = async (row: DvChannel) => {
    if (window.confirm(`确定删除渠道「${row.name}」吗？`)) {
      await domainApi.channels.remove(row.id)
      refresh()
    }
  }

  const columns: CmsColumn<DvChannel>[] = [
    {
      id: 'name',
      header: '渠道',
      render: (r) => (
        <div className="min-w-0">
          <p className="font-medium text-os-text-primary">{r.name}</p>
          {r.url ? <p className="text-xs text-os-text-muted truncate max-w-[200px]">{r.url}</p> : null}
        </div>
      ),
    },
    { id: 'type', header: '类型', render: (r) => <span className="text-os-text-secondary">{r.type}</span> },
    {
      id: 'provider',
      header: 'Provider',
      render: (r) => <span className="text-xs text-os-text-muted">{r.provider || '—'}</span>,
    },
    {
      id: 'contentCount',
      header: '分发内容',
      render: (r) => <span className="text-os-text-secondary tabular-nums">{r.contentCount ?? 0}</span>,
    },
    {
      id: 'status',
      header: '状态',
      render: (r) => (
        <span
          className={`px-1.5 py-0.5 rounded-md text-xs font-medium ${
            r.status === 'active' ? 'bg-emerald-50 text-emerald-600' : 'bg-slate-100 text-slate-500'
          }`}
        >
          {r.status}
        </span>
      ),
    },
    {
      id: 'updatedAt',
      header: '更新时间',
      render: (r) => <time className="text-xs text-os-text-muted">{fmtDate(r.updatedAt)}</time>,
    },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="Domain · 分发渠道" desc="Channel 定义「内容去哪里」：网站 / X / 微信公众号 / Telegram / 邮件… 支持一稿多投。" />
      <CmsToolbar searchPlaceholder="搜索名称 / 类型 / URL…" searchValue={t.search} onSearchChange={t.setSearch}>
        <Auth perm={P.domainChannelCreate}>
          <Button variant="primary" size="sm" onPress={() => { setEditing(null); setModalOpen(true) }}>+ 新建渠道</Button>
        </Auth>
      </CmsToolbar>

      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="📡"
        emptyTitle="还没有分发渠道"
        emptyHint="渠道是内容的出口：创建后即可在内容页一稿多投"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Auth perm={P.domainChannelUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => { setEditing(row); setModalOpen(true) }}>编辑</Button>
            </Auth>
            <Auth perm={P.domainChannelDelete}>
              <Button
                variant="ghost" size="sm"
                className="text-os-danger-text hover:bg-os-danger-bg"
                onPress={() => handleDelete(row)}
              >
                删除
              </Button>
            </Auth>
          </div>
        )}
      />
      <CmsPagination page={t.page} pageCount={t.pageCount} total={t.total} onPageChange={t.setPage} />

      <CmsFormModal
        title={editing ? `编辑渠道：${editing.name}` : '新建渠道'}
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        onSubmit={handleSubmit}
        fields={FORM_FIELDS}
        initial={editing ?? undefined}
      />
    </div>
  )
}

export const Route = createFileRoute('/domain/channels')({
  component: ChannelsPage,
})
