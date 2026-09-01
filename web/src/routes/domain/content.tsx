import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button } from '@heroui/react'

import {
  CONTENT_STATUSES,
  CONTENT_TYPES,
  CONTEXT_ROLES,
  domainApi,
  type DvContent,
} from '../../api/domain'
import type { FormFieldDef } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { ContentStatusBadge } from '../../components/cms/ContentStatusBadge'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

/** 把 /api/v1 的 crud 适配成 useCmsCollection 需要的 CrudService（更新是 PATCH） */
function contentCrud(): import('../../api/cms/store').CrudService<DvContent> {
  return {
    list: () => domainApi.content.list(),
    get: async (id) => {
      try {
        return await domainApi.content.get(id)
      } catch {
        return undefined
      }
    },
    create: async (input) => {
      const id = await domainApi.content.create(input as unknown as Record<string, unknown>)
      return { ...(input as unknown as DvContent), id, createdAt: '', updatedAt: '' }
    },
    update: (id, patch) => domainApi.content.update(id, patch),
    remove: (id) => domainApi.content.remove(id),
  }
}

const FORM_FIELDS: FormFieldDef[] = [
  {
    key: 'type',
    label: '内容类型',
    type: 'select',
    required: true,
    options: CONTENT_TYPES.map((v) => ({ value: v, label: v })),
    defaultValue: 'article',
  },
  { key: 'title', label: '标题', type: 'text', placeholder: '内容标题（profile 可留空）' },
  { key: 'slug', label: 'Slug', type: 'text', placeholder: 'URL 别名，留空自动生成' },
  { key: 'summary', label: '摘要', type: 'textarea', placeholder: '一两句话说明' },
  {
    key: 'status',
    label: '状态',
    type: 'select',
    options: CONTENT_STATUSES.map((v) => ({ value: v, label: v })),
    defaultValue: 'draft',
  },
  { key: 'locale', label: '语言', type: 'text', defaultValue: 'en-US', placeholder: '如 zh-CN / en-US' },
  { key: 'data', label: '数据（JSON）', type: 'textarea', placeholder: '{"body": "...", "metrics": {...}}' },
  { key: 'metadata', label: '元数据（JSON）', type: 'textarea', placeholder: '{"seo": {...}}' },
]

function parseJsonField(v: string | number, key: string): Record<string, unknown> | undefined {
  const s = String(v).trim()
  if (!s) return undefined
  try {
    const parsed = JSON.parse(s)
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) return parsed
    return { [key]: parsed }
  } catch {
    return { [key]: s }
  }
}

function ContentPage() {
  const qc = useQueryClient()
  const [editing, setEditing] = useState<DvContent | null>(null)
  const [modalOpen, setModalOpen] = useState(false)
  const [attachTarget, setAttachTarget] = useState<DvContent | null>(null)
  const [publishTarget, setPublishTarget] = useState<DvContent | null>(null)

  const t = useCmsCollection(contentCrud(), ['domain-content'], {
    searchFields: ['title', 'slug', 'summary', 'type'],
  })

  const contextsQ = useQuery({ queryKey: ['domain-contexts-all'], queryFn: () => domainApi.contexts.list() })
  const channelsQ = useQuery({ queryKey: ['domain-channels-all'], queryFn: () => domainApi.channels.list() })

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ['domain-content'] })
  }

  const openCreate = () => {
    setEditing(null)
    setModalOpen(true)
  }
  const openEdit = (row: DvContent) => {
    setEditing(row)
    setModalOpen(true)
  }

  const handleSubmit = async (values: Record<string, string | number>) => {
    const patch: Record<string, unknown> = {
      type: String(values.type),
      slug: String(values.slug ?? '').trim() || undefined,
      title: String(values.title ?? '').trim() || undefined,
      summary: String(values.summary ?? '').trim() || undefined,
      status: String(values.status),
      locale: String(values.locale ?? 'en-US').trim() || 'en-US',
      data: parseJsonField(values.data ?? '', 'data'),
      metadata: parseJsonField(values.metadata ?? '', 'metadata'),
    }
    if (editing) await domainApi.content.update(editing.id, patch)
    else await domainApi.content.create(patch)
    refresh()
  }

  const handleDelete = async (row: DvContent) => {
    if (window.confirm(`确定删除内容《${row.title ?? row.slug ?? row.id.slice(0, 8)}》吗？`)) {
      await domainApi.content.remove(row.id)
      refresh()
    }
  }

  const handleAttach = async (values: Record<string, string | number>) => {
    if (!attachTarget) return
    await domainApi.content.attachContext(attachTarget.id, {
      contextId: String(values.contextId),
      role: String(values.role ?? 'primary'),
      weight: Number(values.weight ?? 1),
    })
    refresh()
  }

  const handlePublish = async (values: Record<string, string | number>) => {
    if (!publishTarget) return
    await domainApi.content.publishToChannel(publishTarget.id, String(values.channelId), {
      externalUrl: String(values.externalUrl ?? '').trim() || undefined,
    })
    refresh()
  }

  const columns: CmsColumn<DvContent>[] = [
    {
      id: 'title',
      header: '内容',
      render: (r) => (
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="px-1.5 py-0.5 rounded bg-slate-100 text-slate-600 text-xs font-mono">{r.type}</span>
            <p className="font-medium text-os-text-primary truncate max-w-[200px]">{r.title || r.slug || '—'}</p>
          </div>
          <p className="text-xs text-os-text-muted truncate max-w-[260px]">{r.summary || '—'}</p>
        </div>
      ),
    },
    {
      id: 'contexts',
      header: '语义上下文',
      render: (r) =>
        r.contexts && r.contexts.length > 0 ? (
          <div className="flex flex-wrap gap-1 max-w-[220px]">
            {r.contexts.map((c) => (
              <span key={c.id} className="px-1.5 py-0.5 rounded-md bg-violet-50 text-violet-600 text-xs">
                {c.name}
                {c.role ? <span className="opacity-60"> · {c.role}</span> : null}
              </span>
            ))}
          </div>
        ) : (
          <span className="text-os-text-muted text-xs">—</span>
        ),
    },
    {
      id: 'channels',
      header: '分发渠道',
      render: (r) =>
        r.channels && r.channels.length > 0 ? (
          <div className="flex flex-wrap gap-1 max-w-[220px]">
            {r.channels.map((c) => (
              <span key={c.id} className="px-1.5 py-0.5 rounded-md bg-emerald-50 text-emerald-600 text-xs">
                {c.name}
                {c.status === 'published' ? null : <span className="opacity-60"> · {c.status}</span>}
              </span>
            ))}
          </div>
        ) : (
          <span className="text-os-text-muted text-xs">—</span>
        ),
    },
    { id: 'status', header: '状态', render: (r) => <ContentStatusBadge status={r.status as 'draft' | 'published'} /> },
    {
      id: 'updatedAt',
      header: '更新时间',
      render: (r) => <time className="text-xs text-os-text-muted">{fmtDate(r.updatedAt)}</time>,
    },
  ]

  const attachFields: FormFieldDef[] = [
    {
      key: 'contextId',
      label: 'Context',
      type: 'select',
      required: true,
      options: (contextsQ.data ?? []).map((c) => ({ value: c.id, label: `${c.name}（${c.type}）` })),
    },
    {
      key: 'role',
      label: '关系角色',
      type: 'select',
      options: CONTEXT_ROLES.map((v) => ({ value: v, label: v })),
      defaultValue: 'primary',
    },
    { key: 'weight', label: '权重（0-10）', type: 'number', defaultValue: 1 },
  ]

  const publishFields: FormFieldDef[] = [
    {
      key: 'channelId',
      label: '渠道',
      type: 'select',
      required: true,
      options: (channelsQ.data ?? []).map((c) => ({ value: c.id, label: `${c.name}（${c.type}）` })),
    },
    { key: 'externalUrl', label: '外部链接（可选）', type: 'text', placeholder: '如 https://t.me/...' },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="Domain · 内容" desc="业务事实：内容 + 语义上下文（Context）+ 分发渠道（Channel）两个关系维度。" />

      <CmsToolbar searchPlaceholder="搜索标题 / slug / 摘要 / 类型…" searchValue={t.search} onSearchChange={t.setSearch}>
        <Auth perm={P.domainContentCreate}>
          <Button variant="primary" size="sm" onPress={openCreate}>+ 新建内容</Button>
        </Auth>
      </CmsToolbar>

      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="🧬"
        emptyTitle="还没有领域内容"
        emptyHint="内容是一切的中心：可挂载语义上下文、一稿多投到渠道"
        actions={(row) => (
          <div className="flex gap-1.5 flex-wrap">
            <Auth perm={P.domainContentUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => openEdit(row)}>编辑</Button>
            </Auth>
            <Auth perm={P.domainContentUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => setAttachTarget(row)}>挂载 Context</Button>
            </Auth>
            <Auth perm={P.domainContentUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => setPublishTarget(row)}>发布渠道</Button>
            </Auth>
            <Auth perm={P.domainContentDelete}>
              <Button
                variant="ghost"
                size="sm"
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
        title={editing ? `编辑内容：${editing.title || editing.slug}` : '新建内容'}
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        onSubmit={handleSubmit}
        fields={FORM_FIELDS}
        initial={editing ?? undefined}
      />

      <CmsFormModal
        title={attachTarget ? `挂载 Context → ${attachTarget.title || attachTarget.slug}` : '挂载 Context'}
        isOpen={!!attachTarget}
        onClose={() => setAttachTarget(null)}
        onSubmit={handleAttach}
        fields={attachFields}
        submitLabel="挂载"
      />

      <CmsFormModal
        title={publishTarget ? `分发到渠道 → ${publishTarget.title || publishTarget.slug}` : '发布渠道'}
        isOpen={!!publishTarget}
        onClose={() => setPublishTarget(null)}
        onSubmit={handlePublish}
        fields={publishFields}
        submitLabel="分发"
      />
      {publishTarget && publishTarget.status !== 'published' ? (
        <p className="text-xs text-amber-600 px-1 -mt-1">
          ⚠️ 该内容状态为「{publishTarget.status}」，分发前需先发布内容（后端会校验）。
        </p>
      ) : null}
    </div>
  )
}

export const Route = createFileRoute('/domain/content')({
  component: ContentPage,
})
