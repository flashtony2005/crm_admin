import { useState } from 'react'
import { Button } from '@heroui/react'

import { leadsApi, type Lead, type LeadStatus } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import type { FormFieldDef } from '../../api/cms'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { StatusBadge, type StatusTone } from '../../components/common/StatusBadge'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

import { createFileRoute } from '@tanstack/react-router'

const LEAD_STATUS: Record<LeadStatus, { label: string; tone: StatusTone }> = {
  new: { label: '新线索', tone: 'purple' },
  following: { label: '跟进中', tone: 'info' },
  won: { label: '已成交', tone: 'success' },
  lost: { label: '已流失', tone: 'neutral' },
}

/** 线索状态流转：新 → 跟进中 → 已成交（点击徽章推进） */
const NEXT_STATUS: Partial<Record<LeadStatus, LeadStatus>> = {
  new: 'following',
  following: 'won',
}

const FORM_FIELDS: FormFieldDef[] = [
  { key: 'name', label: '线索名称 / 联系人', type: 'text', required: true },
  { key: 'phone', label: '电话', type: 'text' },
  { key: 'interest', label: '意向内容', type: 'textarea', placeholder: '想买什么、数量、时间…' },
  { key: 'source', label: '来源', type: 'text', defaultValue: '手动录入' },
]

function LeadsPage() {
  const [modalOpen, setModalOpen] = useState(false)

  const t = useCmsCollection(leadsApi, ['cms-leads'], {
    searchFields: ['name', 'interest', 'source'],
  })

  const advance = (row: Lead) => {
    const next = NEXT_STATUS[row.status]
    if (!next || !window.confirm(`将「${row.name}」推进到「${LEAD_STATUS[next].label}」？`)) return
    t.update.mutateAsync({ id: row.id, patch: { status: next } })
  }

  const handleSubmit = async (values: Record<string, string | number>) => {
    await t.create.mutateAsync({
      name: String(values.name),
      phone: String(values.phone),
      interest: String(values.interest),
      source: String(values.source),
      status: 'new' as LeadStatus,
    })
  }

  const columns: CmsColumn<Lead>[] = [
    {
      id: 'name',
      header: '线索',
      render: (r) => (
        <div>
          <p className="font-medium text-os-text-primary">{r.name}</p>
          <p className="text-xs text-os-text-muted font-mono">{r.phone}</p>
        </div>
      ),
    },
    {
      id: 'interest',
      header: '意向',
      cellClassName: 'max-w-[260px]',
      render: (r) => <span className="text-sm text-os-text-secondary">{r.interest}</span>,
    },
    {
      id: 'status',
      header: '状态',
      render: (r) => (
        <span
          role="button"
          tabIndex={0}
          onClick={() => advance(r)}
          onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && advance(r)}
          title={NEXT_STATUS[r.status] ? '点击推进到下一阶段' : undefined}
          className={`inline-flex ${NEXT_STATUS[r.status] ? 'cursor-pointer' : ''}`}
        >
          <StatusBadge tone={LEAD_STATUS[r.status].tone} dot>{LEAD_STATUS[r.status].label}</StatusBadge>
        </span>
      ),
    },
    { id: 'source', header: '来源', render: (r) => <span className="text-os-text-secondary">{r.source}</span> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="Leads" desc="潜在客户线索：新 → 跟进中 → 已成交，点击状态可推进。" />
      <CmsToolbar searchPlaceholder="搜索线索…" searchValue={t.search} onSearchChange={t.setSearch}>
        <Auth perm={P.businessLeadsCreate}>
          <Button variant="primary" size="sm" onPress={() => setModalOpen(true)}>
            + 新建线索
          </Button>
        </Auth>
      </CmsToolbar>

      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="🎯"
        emptyTitle="还没有线索"
        emptyHint="把表单分享出去，或手动录入第一条线索"
        actions={(row) => (
          <Button
            variant="ghost" size="sm"
            className="text-os-danger-text hover:bg-os-danger-bg"
            onPress={() => window.confirm(`确定删除「${row.name}」吗？`) && t.remove.mutateAsync(row.id)}
          >
            删除
          </Button>
        )}
      />
      <CmsPagination page={t.page} pageCount={t.pageCount} total={t.total} onPageChange={t.setPage} />

      <CmsFormModal
        title="新建线索"
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        onSubmit={handleSubmit}
        fields={FORM_FIELDS}
      />
    </div>
  )
}

export const Route = createFileRoute('/business/leads')({
  component: LeadsPage,
})
