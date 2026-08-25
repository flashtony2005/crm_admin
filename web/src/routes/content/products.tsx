import { useState } from 'react'
import { Button } from '@heroui/react'

import { productsApi, type FormFieldDef, type Product } from '../../api/cms'
import { CONTENT_STATUS_OPTIONS } from '../../components/cms/contentStatus'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { ContentStatusBadge } from '../../components/cms/ContentStatusBadge'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

import { createFileRoute } from '@tanstack/react-router'

const FORM_FIELDS: FormFieldDef[] = [
  { key: 'name', label: '产品名称', type: 'text', required: true, placeholder: '例如：桂花栗子欧包' },
  { key: 'sku', label: 'SKU 编码', type: 'text', placeholder: 'BREAD-001（留空自动生成）' },
  { key: 'price', label: '价格（元）', type: 'number', required: true, defaultValue: 0 },
  { key: 'stock', label: '库存', type: 'number', defaultValue: 0 },
  { key: 'status', label: '状态', type: 'select', options: CONTENT_STATUS_OPTIONS, defaultValue: 'draft' },
]

function ProductsPage() {
  const [editing, setEditing] = useState<Product | null>(null)
  const [modalOpen, setModalOpen] = useState(false)

  const t = useCmsCollection(productsApi, ['cms-products'], {
    searchFields: ['name', 'sku'],
  })

  const handleSubmit = async (values: Record<string, string | number>) => {
    const patch = {
      name: String(values.name),
      sku: String(values.sku) || `SKU-${Date.now().toString(36).toUpperCase()}`,
      price: Number(values.price) || 0,
      stock: Number(values.stock) || 0,
      status: values.status as Product['status'],
    }
    if (editing) await t.update.mutateAsync({ id: editing.id, patch })
    else await t.create.mutateAsync(patch)
  }

  const columns: CmsColumn<Product>[] = [
    {
      id: 'name',
      header: '产品',
      render: (r) => (
        <div>
          <p className="font-medium text-os-text-primary">{r.name}</p>
          <p className="text-xs text-os-text-muted font-mono">{r.sku}</p>
        </div>
      ),
    },
    {
      id: 'price',
      header: '价格',
      render: (r) => <span className="tabular-nums font-medium">¥{r.price.toFixed(2)}</span>,
    },
    {
      id: 'stock',
      header: '库存',
      render: (r) => (
        <span className={`tabular-nums ${r.stock <= 10 ? 'text-orange-500 font-medium' : 'text-os-text-secondary'}`}>
          {r.stock}
        </span>
      ),
    },
    { id: 'status', header: '状态', render: (r) => <ContentStatusBadge status={r.status} /> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="Products" desc="你的产品与服务，上架后同步到网站和小程序。" />
      <CmsToolbar searchPlaceholder="搜索产品 / SKU…" searchValue={t.search} onSearchChange={t.setSearch}>
        <Auth perm={P.contentProductsCreate}>
          <Button
            variant="primary" size="sm"
            onPress={() => { setEditing(null); setModalOpen(true) }}
          >
            + 新建产品
          </Button>
        </Auth>
      </CmsToolbar>

      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="📦"
        emptyTitle="还没有产品"
        emptyHint="点击「新建产品」添加第一个产品或服务"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Auth perm={P.contentProductsUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => { setEditing(row); setModalOpen(true) }}>编辑</Button>
            </Auth>
            <Auth perm={P.contentProductsDelete}>
              <Button
                variant="ghost" size="sm"
                className="text-os-danger-text hover:bg-os-danger-bg"
                onPress={() => window.confirm(`确定删除「${row.name}」吗？`) && t.remove.mutateAsync(row.id)}
              >
                删除
              </Button>
            </Auth>
          </div>
        )}
      />
      <CmsPagination page={t.page} pageCount={t.pageCount} total={t.total} onPageChange={t.setPage} />

      <CmsFormModal
        title={editing ? `编辑产品：${editing.name}` : '新建产品'}
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        onSubmit={handleSubmit}
        fields={FORM_FIELDS}
        initial={editing ?? undefined}
      />
    </div>
  )
}

export const Route = createFileRoute('/content/products')({
  component: ProductsPage,
})
