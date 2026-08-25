import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { Button } from '@heroui/react'

import { customersApi, type Customer, type CustomerPriority, type FormFieldDef } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtRelative } from '../../components/cms/format'
import { StatusBadge } from '../../components/common/StatusBadge'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'


const PRIORITY_META: Record<CustomerPriority, { label: string; tone: 'danger' | 'warning' | 'neutral' }> = {
  high: { label: '高优先级', tone: 'danger' },
  mid: { label: '中', tone: 'warning' },
  low: { label: '低', tone: 'neutral' },
}

const FORM_FIELDS: FormFieldDef[] = [
  { key: 'name', label: '客户名称', type: 'text', required: true },
  { key: 'phone', label: '电话', type: 'text' },
  {
    key: 'source', label: '来源', type: 'select', defaultValue: '到店扫码',
    options: ['到店扫码', '小程序', '大众点评', '小红书', '老客推荐', '表单'].map((v) => ({ value: v, label: v })),
  },
  {
    key: 'priority', label: '优先级', type: 'select', defaultValue: 'mid',
    options: [
      { value: 'high', label: '高优先级' },
      { value: 'mid', label: '中' },
      { value: 'low', label: '低' },
    ],
  },
  { key: 'note', label: '备注', type: 'textarea', placeholder: '需求、偏好、跟进记录…' },
]

function CustomersPage() {
  const [editing, setEditing] = useState<Customer | null>(null)
  const [modalOpen, setModalOpen] = useState(false)

  const t = useCmsCollection(customersApi, ['cms-customers'], {
    searchFields: ['name', 'phone', 'note'],
  })

  const handleSubmit = async (values: Record<string, string | number>) => {
    const patch = {
      name: String(values.name),
      phone: String(values.phone),
      source: String(values.source),
      priority: values.priority as Customer['priority'],
      note: String(values.note),
    }
    if (editing) await t.update.mutateAsync({ id: editing.id, patch })
    else
      await t.create.mutateAsync({
        ...patch,
        tags: [],
        lastContactAt: new Date().toISOString(),
      })
  }

  const columns: CmsColumn<Customer>[] = [
    {
      id: 'name',
      header: '客户',
      render: (r) => (
        <div>
          <p className="font-medium text-os-text-primary">{r.name}</p>
          <p className="text-xs text-os-text-muted font-mono">{r.phone || '—'}</p>
        </div>
      ),
    },
    {
      id: 'priority',
      header: '优先级',
      render: (r) => {
        const m = PRIORITY_META[r.priority]
        return <StatusBadge tone={m.tone} dot>{m.label}</StatusBadge>
      },
    },
    {
      id: 'tags',
      header: '标签',
      render: (r) => (
        <div className="flex gap-1 flex-wrap">
          {r.tags.length === 0 ? '—' : r.tags.map((tag) => (
            <span key={tag} className="text-xs px-1.5 py-0.5 rounded bg-gray-100 text-gray-500">{tag}</span>
          ))}
        </div>
      ),
    },
    { id: 'source', header: '来源', render: (r) => <span className="text-os-text-secondary">{r.source}</span> },
    { id: 'note', header: '备注', cellClassName: 'max-w-[220px]', render: (r) => <span className="text-xs text-os-text-muted line-clamp-2">{r.note || '—'}</span> },
    { id: 'lastContactAt', header: '最近联系', render: (r) => <time className="text-xs text-os-text-muted">{fmtRelative(r.lastContactAt)}</time> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="Customers" desc="客户档案：谁在买、买过什么、下一步该做什么。" />
      <CmsToolbar searchPlaceholder="搜索姓名 / 电话 / 备注…" searchValue={t.search} onSearchChange={t.setSearch}>
        <Auth perm={P.businessCustomersCreate}>
          <Button variant="primary" size="sm" onPress={() => { setEditing(null); setModalOpen(true) }}>
            + 新建客户
          </Button>
        </Auth>
      </CmsToolbar>

      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="👥"
        emptyTitle="还没有客户档案"
        emptyHint="点击「新建客户」，或把门店表单接入后自动收集"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Auth perm={P.businessCustomersUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => { setEditing(row); setModalOpen(true) }}>编辑</Button>
            </Auth>
            <Auth perm={P.businessCustomersDelete}>
              <Button
                variant="ghost" size="sm"
                className="text-os-danger-text hover:bg-os-danger-bg"
                onPress={() => window.confirm(`确定删除客户「${row.name}」吗？`) && t.remove.mutateAsync(row.id)}
              >
                删除
              </Button>
            </Auth>
          </div>
        )}
      />
      <CmsPagination page={t.page} pageCount={t.pageCount} total={t.total} onPageChange={t.setPage} />

      <CmsFormModal
        title={editing ? `编辑客户：${editing.name}` : '新建客户'}
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        onSubmit={handleSubmit}
        fields={FORM_FIELDS}
        initial={editing ?? undefined}
      />
    </div>
  )
}

export const Route = createFileRoute('/business/customers')({
  component: CustomersPage,
})
