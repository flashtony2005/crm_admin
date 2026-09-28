import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsToolbar } from '../../components/cms/CmsToolbar'
import { fmtDate } from '../../components/cms/format'
import { subscriptionsListApi, type SubscriptionRecord } from '../../api/cms'

/**
 * Stripe 状态 → 展示样式。
 *
 * 未知状态**原样显示 Stripe 的原话**：编一个中文名会让人以为后端已经处理了它，
 * 而实际语义恰恰相反 —— `sub_state::SubStatus` 对看不懂的状态一律不发服务。
 * 原样显示，运营才知道要去 Stripe 后台看一眼。
 */
function statusStyle(s: string): { label: string; cls: string } {
  switch (s) {
    case 'active':
      return { label: '计费中', cls: 'bg-emerald-50 text-emerald-600' }
    case 'trialing':
      return { label: '试用期', cls: 'bg-blue-50 text-blue-600' }
    case 'past_due':
      return { label: '扣款失败·重试中', cls: 'bg-amber-50 text-amber-600' }
    case 'canceled':
      return { label: '已取消', cls: 'bg-gray-100 text-gray-600' }
    case 'unpaid':
      return { label: '未支付', cls: 'bg-red-50 text-red-600' }
    case 'incomplete':
    case 'incomplete_expired':
      return { label: '未完成', cls: 'bg-red-50 text-red-600' }
    default:
      return { label: s || '未知', cls: 'bg-gray-100 text-gray-500' }
  }
}

function SubscriptionRecordsPage() {
  const [search, setSearch] = useState('')
  const t = useCmsCollection(subscriptionsListApi, ['cms-subscriptions'], {
    searchFields: ['memberId', 'stripeSubscriptionId', 'status', 'tierSlug'],
    serverPaged: true,
  })

  const columns: CmsColumn<SubscriptionRecord>[] = [
    {
      id: 'memberId',
      header: '会员',
      render: (r) => <span className="font-mono text-xs">{r.memberId?.slice(0, 12) || '—'}</span>,
    },
    {
      id: 'tierSlug',
      header: '套餐 / 周期',
      render: (r) => (
        <div className="min-w-0">
          <p className="text-sm">{r.tierSlug || '付费'}</p>
          <p className="text-xs text-os-text-muted">{r.interval === 'yearly' ? '年付' : '月付'}</p>
        </div>
      ),
    },
    {
      id: 'status',
      header: 'Stripe 状态',
      render: (r) => {
        const m = statusStyle(r.status)
        return <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${m.cls}`}>{m.label}</span>
      },
    },
    {
      id: 'currentPeriodEnd',
      header: '当前周期末',
      // 这一列是整页存在的理由：**谁快到期、谁已经到期但还没被兜底处理**，
      // 必须一眼可见。此前这两个问题只能去 Stripe 后台翻。
      render: (r) => {
        if (!r.currentPeriodEnd) return <span className="text-xs text-os-text-muted">—</span>
        const days = Math.ceil((new Date(r.currentPeriodEnd).getTime() - Date.now()) / 86_400_000)
        return (
          <div className="min-w-0">
            <p className="text-sm">{fmtDate(r.currentPeriodEnd)}</p>
            {/* 已终结（closed_at 非空）就不再提示「待兜底降级」—— 兜底早就
                处理过它了，再说待处理是自相矛盾；终结与否由「续费」列统一表达。 */}
            {!r.closedAt && (
              <p
                className={`text-xs ${
                  days <= 0 ? 'text-red-600' : days <= 7 ? 'text-amber-600' : 'text-os-text-muted'
                }`}
              >
                {days <= 0 ? '已到期（待兜底降级）' : `还剩 ${days} 天`}
              </p>
            )}
          </div>
        )
      },
    },
    {
      id: 'cancelAtPeriodEnd',
      header: '续费',
      render: (r) =>
        r.closedAt ? (
          <span className="text-xs text-os-text-muted">已终结</span>
        ) : r.cancelAtPeriodEnd ? (
          <span className="rounded-full bg-amber-50 px-2 py-0.5 text-xs font-medium text-amber-600">
            到期后不再续费
          </span>
        ) : (
          <span className="text-xs text-os-text-muted">到期自动续费</span>
        ),
    },
    {
      id: 'startedAt',
      header: '订阅开始',
      render: (r) => (
        <span className="text-xs text-os-text-muted">{r.startedAt ? fmtDate(r.startedAt) : '—'}</span>
      ),
    },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader
        title="订阅记录"
        desc="Stripe 订阅在本地的事实源：续费周期、取消状态、到期降级都由 webhook 状态机写入，后台只读。到期未续的订阅由调度器每分钟兜底降级，即使 webhook 丢失也不会一直有效。"
      />
      <CmsToolbar
        searchPlaceholder="搜索会员 / 订阅号 / 状态…"
        searchValue={search}
        onSearchChange={setSearch}
      >
        <span className="text-xs text-os-text-muted">共 {t.paged.length} 条</span>
      </CmsToolbar>
      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="🔁"
        emptyTitle="还没有订阅记录"
        emptyHint="会员通过「在线支付」创建订阅后，Stripe 的事件会在这里生成记录"
      />
    </div>
  )
}

export const Route = createFileRoute('/content/subscription-records')({
  component: SubscriptionRecordsPage,
})
