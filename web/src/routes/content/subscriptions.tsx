import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { Button } from '@heroui/react'
import { tiersApi, type FormFieldDef, type Tier } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

const FORM_FIELDS: FormFieldDef[] = [
  { key: 'name', label: '套餐名', type: 'text', required: true, placeholder: '月度会员' },
  { key: 'slug', label: 'Slug', type: 'text', placeholder: 'monthly' },
  { key: 'description', label: '描述', type: 'textarea', placeholder: '套餐权益说明' },
  { key: 'priceMonthly', label: '月价', type: 'number', placeholder: '18' },
  { key: 'priceYearly', label: '年价', type: 'number', placeholder: '180' },
  { key: 'stripePriceId', label: 'Stripe Price ID', type: 'text', placeholder: 'price_xxx（留空进入测试模式）' },
  { key: 'features', label: '权益（JSON 数组）', type: 'textarea', placeholder: '["专属内容","徽章"]' },
]

function SubscriptionsPage() {
  const [editing, setEditing] = useState<Tier | null>(null)
  const [open, setOpen] = useState(false)
  const t = useCmsCollection(tiersApi, ['cms-tiers'], { searchFields: ['name', 'slug'] })

  const submit = async (v: Record<string, string | number>) => {
    const patch = {
      name: String(v.name),
      slug: String(v.slug ?? '').trim(),
      description: String(v.description ?? ''),
      priceMonthly: Number(v.priceMonthly ?? 0),
      priceYearly: Number(v.priceYearly ?? 0),
      stripePriceId: String(v.stripePriceId ?? '').trim(),
      features: String(v.features ?? '[]'),
      active: true,
    }
    if (editing) await t.update.mutateAsync({ id: editing.id, patch })
    else await t.create.mutateAsync(patch)
  }

  const columns: CmsColumn<Tier>[] = [
    { id: 'name', header: '套餐', render: (r) => <span className="font-medium">{r.name}</span> },
    { id: 'price', header: '价格', render: (r) => <span>¥{r.priceMonthly}/月 · ¥{r.priceYearly}/年</span> },
    { id: 'stripe', header: 'Stripe', render: (r) => <span className="text-xs text-os-text-muted">{r.stripePriceId || '测试模式'}</span> },
    { id: 'active', header: '启用', render: (r) => <span>{r.active ? '是' : '否'}</span> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="付费订阅" desc="套餐（Tiers）管理。配置 STRIPE_SECRET_KEY 后，会员在公开站发起 Checkout 即为真实扣费。" />
      <CmsToolbar searchPlaceholder="搜索套餐…" searchValue={t.search} onSearchChange={t.setSearch}>
        <Auth perm={P.subscriptionsTiersCreate}>
          <Button variant="primary" size="sm" onPress={() => { setEditing(null); setOpen(true) }}>+ 新建套餐</Button>
        </Auth>
      </CmsToolbar>
      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="💳"
        emptyTitle="还没有套餐"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Auth perm={P.subscriptionsTiersUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => { setEditing(row); setOpen(true) }}>编辑</Button>
            </Auth>
            <Auth perm={P.subscriptionsTiersDelete}>
              <Button variant="ghost" size="sm" className="text-red-500" onPress={() => window.confirm('删除该套餐？') && t.remove.mutateAsync(row.id)}>删除</Button>
            </Auth>
          </div>
        )}
      />
      <CmsFormModal
        isOpen={open} onClose={() => setOpen(false)}
        title={editing ? `编辑套餐 · ${editing.name}` : '新建套餐'}
        fields={FORM_FIELDS} initial={editing ?? undefined}
        onSubmit={async (v) => { await submit(v); setOpen(false) }}
      />
    </div>
  )
}

export const Route = createFileRoute('/content/subscriptions')({ component: SubscriptionsPage })
