import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Button } from '@heroui/react'

import { CONTEXT_TYPES, domainApi, type DvContext } from '../../api/domain'
import type { FormFieldDef } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

function contextCrud(): import('../../api/cms/store').CrudService<DvContext> {
  return {
    list: () => domainApi.contexts.list(),
    get: async (id) => {
      try {
        return await domainApi.contexts.get(id)
      } catch {
        return undefined
      }
    },
    create: async (input) => {
      const id = await domainApi.contexts.create(input as unknown as Record<string, unknown>)
      return { ...(input as unknown as DvContext), id, createdAt: '', updatedAt: '' }
    },
    update: (id, patch) => domainApi.contexts.update(id, patch),
    remove: (id) => domainApi.contexts.remove(id),
  }
}

const FORM_FIELDS: FormFieldDef[] = [
  { key: 'name', label: '名称', type: 'text', required: true, placeholder: '如：AI / Web3 / Creator' },
  { key: 'slug', label: 'Slug', type: 'text', placeholder: '留空按名称自动生成' },
  {
    key: 'type',
    label: '类型',
    type: 'select',
    options: CONTEXT_TYPES.map((v) => ({ value: v, label: v })),
    defaultValue: 'topic',
  },
  { key: 'description', label: '描述', type: 'textarea', placeholder: '这个上下文的语义是什么' },
  {
    key: 'status',
    label: '状态',
    type: 'select',
    options: ['active', 'inactive'].map((v) => ({ value: v, label: v })),
    defaultValue: 'active',
  },
  { key: 'data', label: '语义数据（JSON）', type: 'textarea', placeholder: '{"audience": "builder", "intent": "education"}' },
]

function ContextsPage() {
  const qc = useQueryClient()
  const [editing, setEditing] = useState<DvContext | null>(null)
  const [modalOpen, setModalOpen] = useState(false)

  const t = useCmsCollection(contextCrud(), ['domain-contexts'], {
    searchFields: ['name', 'slug', 'type', 'description'],
  })

  const refresh = () => qc.invalidateQueries({ queryKey: ['domain-contexts'] })

  const handleSubmit = async (values: Record<string, string | number>) => {
    const patch: Record<string, unknown> = {
      name: String(values.name),
      slug: String(values.slug ?? '').trim() || undefined,
      type: String(values.type ?? 'topic'),
      description: String(values.description ?? '').trim() || undefined,
      status: String(values.status ?? 'active'),
      data: (() => {
        const s = String(values.data ?? '').trim()
        if (!s) return undefined
        try {
          const p = JSON.parse(s)
          return p && typeof p === 'object' && !Array.isArray(p) ? p : { data: s }
        } catch {
          return { data: s }
        }
      })(),
    }
    if (editing) await domainApi.contexts.update(editing.id, patch)
    else await domainApi.contexts.create(patch)
    refresh()
  }

  const handleDelete = async (row: DvContext) => {
    if (window.confirm(`确定删除上下文「${row.name}」吗？`)) {
      await domainApi.contexts.remove(row.id)
      refresh()
    }
  }

  const columns: CmsColumn<DvContext>[] = [
    {
      id: 'name',
      header: '上下文',
      render: (r) => (
        <div className="min-w-0">
          <p className="font-medium text-os-text-primary">{r.name}</p>
          <p className="text-xs text-os-text-muted truncate max-w-[220px]">{r.description || `/${r.slug}`}</p>
        </div>
      ),
    },
    { id: 'type', header: '类型', render: (r) => <span className="text-os-text-secondary">{r.type}</span> },
    {
      id: 'data',
      header: '语义数据',
      render: (r) => {
        const keys = Object.keys(r.data ?? {}).slice(0, 3)
        return keys.length ? (
          <span className="text-xs text-os-text-muted">{keys.join(' · ')}</span>
        ) : (
          <span className="text-os-text-muted text-xs">—</span>
        )
      },
    },
    {
      id: 'contentCount',
      header: '关联内容',
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
      <CmsPageHeader title="Domain · 语义上下文" desc="Context 是 Agent 理解内容的语义维度 —— 不是标签，而是「谁在看 / 为什么看 / 属于什么主题」。" />
      <CmsToolbar searchPlaceholder="搜索名称 / slug / 类型…" searchValue={t.search} onSearchChange={t.setSearch}>
        <Auth perm={P.domainContextCreate}>
          <Button variant="primary" size="sm" onPress={() => { setEditing(null); setModalOpen(true) }}>+ 新建上下文</Button>
        </Auth>
      </CmsToolbar>

      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="🧠"
        emptyTitle="还没有语义上下文"
        emptyHint="上下文把内容按语义维度组织起来，AI 才能精准理解与检索"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Auth perm={P.domainContextUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => { setEditing(row); setModalOpen(true) }}>编辑</Button>
            </Auth>
            <Auth perm={P.domainContextDelete}>
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
        title={editing ? `编辑上下文：${editing.name}` : '新建上下文'}
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        onSubmit={handleSubmit}
        fields={FORM_FIELDS}
        initial={editing ?? undefined}
      />
    </div>
  )
}

export const Route = createFileRoute('/domain/contexts')({
  component: ContextsPage,
})
