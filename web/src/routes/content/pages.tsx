import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { Button } from '@heroui/react'

import { pagesApi, type FormFieldDef, type Page } from '../../api/cms'
import { CONTENT_STATUS_OPTIONS } from '../../components/cms/contentStatus'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { ContentStatusBadge } from '../../components/cms/ContentStatusBadge'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'


const FORM_FIELDS: FormFieldDef[] = [
  { key: 'title', label: '页面标题', type: 'text', required: true, placeholder: '例如：关于我们' },
  { key: 'path', label: '访问路径', type: 'text', required: true, placeholder: '/about' },
  { key: 'status', label: '状态', type: 'select', options: CONTENT_STATUS_OPTIONS, defaultValue: 'draft' },
]

function PagesPage() {
  const [editing, setEditing] = useState<Page | null>(null)
  const [modalOpen, setModalOpen] = useState(false)

  const t = useCmsCollection(pagesApi, ['cms-pages'], {
    searchFields: ['title', 'path'],
  })

  const handleSubmit = async (values: Record<string, string | number>) => {
    const patch = {
      title: String(values.title),
      path: String(values.path),
      status: values.status as Page['status'],
    }
    if (editing) await t.update.mutateAsync({ id: editing.id, patch })
    else await t.create.mutateAsync({ ...patch, views: 0 })
  }

  const columns: CmsColumn<Page>[] = [
    {
      id: 'title',
      header: '页面',
      render: (r) => (
        <div>
          <p className="font-medium text-os-text-primary">{r.title}</p>
          <p className="text-xs text-os-text-muted font-mono">{r.path}</p>
        </div>
      ),
    },
    { id: 'status', header: '状态', render: (r) => <ContentStatusBadge status={r.status} /> },
    { id: 'views', header: '访问量', render: (r) => <span className="text-os-text-secondary tabular-nums">{r.views}</span> },
    { id: 'updatedAt', header: '更新时间', render: (r) => <time className="text-xs text-os-text-muted">{fmtDate(r.updatedAt)}</time> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="Pages" desc="网站的固定页面，如首页、关于我们、菜单。" />
      <CmsToolbar searchPlaceholder="搜索页面…" searchValue={t.search} onSearchChange={t.setSearch}>
        <Auth perm={P.contentPagesCreate}>
          <Button
            variant="primary" size="sm"
            onPress={() => { setEditing(null); setModalOpen(true) }}
          >
            + 新建页面
          </Button>
        </Auth>
      </CmsToolbar>

      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="📄"
        emptyTitle="还没有页面"
        emptyHint="点击「新建页面」，或选择行业模板自动生成"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Auth perm={P.contentPagesUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => { setEditing(row); setModalOpen(true) }}>编辑</Button>
            </Auth>
            <Auth perm={P.contentPagesDelete}>
              <Button
                variant="ghost" size="sm"
                className="text-os-danger-text hover:bg-os-danger-bg"
                onPress={() => window.confirm(`确定删除页面「${row.title}」吗？`) && t.remove.mutateAsync(row.id)}
              >
                删除
              </Button>
            </Auth>
          </div>
        )}
      />
      <CmsPagination page={t.page} pageCount={t.pageCount} total={t.total} onPageChange={t.setPage} />

      <CmsFormModal
        title={editing ? `编辑页面：${editing.title}` : '新建页面'}
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        onSubmit={handleSubmit}
        fields={FORM_FIELDS}
        initial={editing ?? undefined}
      />
    </div>
  )
}

export const Route = createFileRoute('/content/pages')({
  component: PagesPage,
})
