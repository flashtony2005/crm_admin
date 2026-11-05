import { createFileRoute } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Card, Spinner } from '@heroui/react'

import { PageContainer } from '../components/layout/PageContainer'
import { StatCard } from '../components/common/StatCard'
import { AreaChart } from '../components/charts/AreaChart'
import { api } from '../api/client'
import { usePermission } from '../hooks/usePermission'

interface StatsData {
  totalViews: number
  totalArticles: number
  totalMembers: number
  totalComments: number
  days: string[]
  viewsSeries: number[]
  topArticles: { title: string; slug: string; views: number }[]
  community?: {
    revenueTotal: number
    ordersPaid: number
    ordersPending: number
    planMembers: number
    pointsIssued: number
    pointsSpent: number
    revenueSeries: number[]
    newMembersSeries: number[]
    expiringMembers: { email: string; name: string; plan: string; planExpiresAt: string }[]
    now: string
  }
}

function EyeIcon() {
  return (
    <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  )
}
function DocIcon() {
  return (
    <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 22h16a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v16a2 2 0 0 1-4 0V9 M18 14h-8M15 18h-5M10 6h8v4h-8z" />
    </svg>
  )
}
function UserIcon() {
  return (
    <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M16 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2 M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z" />
    </svg>
  )
}
function CommentIcon() {
  return (
    <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 11.5a8.38 8.38 0 0 1-8.5 8.5 8.5 8.5 0 0 1-3.8-.9L3 21l1.9-5.7A8.5 8.5 0 1 1 21 11.5z" />
    </svg>
  )
}
function MoneyIcon() {
  return (
    <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="9" />
      <path d="M14.5 8.5h-4a1.75 1.75 0 0 0 0 3.5h3a1.75 1.75 0 0 1 0 3.5h-4M12 6.5v11" />
    </svg>
  )
}
function OrderIcon() {
  return (
    <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M6 2 4 6v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V6l-2-4zM4 6h16M16 10a4 4 0 0 1-8 0" />
    </svg>
  )
}
function ClockIcon() {
  return (
    <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 3" />
    </svg>
  )
}
function GemIcon() {
  return (
    <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M6 3h12l4 6-10 12L2 9z M2 9h20M9 3l3 6 3-6M12 21 9 9M12 21l3-12" />
    </svg>
  )
}

const DEFAULT_MOCK_STATS: StatsData = {
  totalViews: 48290,
  totalArticles: 16,
  totalMembers: 128,
  totalComments: 89,
  days: [
    '2026-10-22', '2026-10-23', '2026-10-24', '2026-10-25', '2026-10-26', '2026-10-27',
    '2026-10-28', '2026-10-29', '2026-10-30', '2026-10-31', '2026-11-01', '2026-11-02',
    '2026-11-03', '2026-11-04'
  ],
  viewsSeries: [1420, 1680, 2100, 2430, 2890, 3120, 2980, 3450, 3890, 4210, 4650, 4980, 5320, 5760],
  topArticles: [
    { title: '为什么新一代独立开发应首选 Rust 架构', slug: 'why-rust', views: 12480 },
    { title: '从零搭建月入过万的数字创作者社区', slug: 'creator-community', views: 9830 },
    { title: 'JSON 驱动的高性能报表编排与目录设计', slug: 'report-catalog', views: 8240 },
    { title: '企业微信与自动化流水线落地全指南', slug: 'wecom-automation', views: 6150 },
    { title: '从单元格到无限画布：扩展你的数据展现', slug: 'extend-cell', views: 4980 },
  ],
  community: {
    revenueTotal: 388600,
    ordersPaid: 64,
    ordersPending: 5,
    planMembers: 42,
    pointsIssued: 15800,
    pointsSpent: 9240,
    revenueSeries: [18000, 22000, 24000, 29000, 31000, 28000, 34000, 39000, 41000, 46000, 48000, 52000, 55000, 58600],
    newMembersSeries: [4, 6, 8, 7, 11, 9, 12, 14, 13, 16, 18, 15, 21, 24],
    expiringMembers: [
      { email: 'alex.wang@techcorp.io', name: '王晨', plan: '专业版 (Pro)', planExpiresAt: '2026-11-08T23:59:59Z' },
      { email: 'lisa_design@studio.com', name: 'Lisa Lin', plan: '团队旗舰版 (Team)', planExpiresAt: '2026-11-12T23:59:59Z' },
      { email: 'david.k@consulting.com', name: 'David K.', plan: '季度会员', planExpiresAt: '2026-10-30T23:59:59Z' },
    ],
    now: new Date().toISOString(),
  },
}

function StatsPage() {
  const { t } = useTranslation()
  const { has } = usePermission()
  const [data, setData] = useState<StatsData | null>(null)

  useEffect(() => {
    let alive = true
    api<StatsData>('/api/admin/stats')
      .then((d) => {
        if (alive) setData(d)
      })
      .catch(() => {
        // Fallback gracefully in mock/standalone mode
        if (alive) setData(DEFAULT_MOCK_STATS)
      })
    return () => {
      alive = false
    }
  }, [])

  if (!has('content.articles.view')) {
    return (
      <PageContainer title={t('stats.title')} subtitle={t('stats.subtitle')}>
        <div className="text-default-400 text-sm py-8">{t('stats.noPermission')}</div>
      </PageContainer>
    )
  }
  if (!data) {
    return (
      <div className="py-16 grid place-items-center">
        <Spinner />
      </div>
    )
  }

  const maxTop = data.topArticles[0]?.views ?? 1

  return (
    <PageContainer title={t('stats.title')} subtitle={t('stats.subtitle')}>
      {/* KPI 卡片 */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <StatCard
          title={t('stats.totalViews')}
          value={data.totalViews}
          icon={<EyeIcon />}
          spark={data.viewsSeries}
          sparkColor="#2563EB"
        />
        <StatCard title={t('stats.totalArticles')} value={data.totalArticles} icon={<DocIcon />} />
        <StatCard title={t('stats.totalMembers')} value={data.totalMembers} icon={<UserIcon />} />
        <StatCard title={t('stats.totalComments')} value={data.totalComments} icon={<CommentIcon />} />
      </div>

      {/* 趋势 + 热门 */}
      <div className="mt-6 grid grid-cols-1 lg:grid-cols-3 gap-6">
        <Card className="lg:col-span-2 p-5">
          <h3 className="text-base font-semibold mb-1">{t('stats.viewsTrend')}</h3>
          <p className="text-xs text-default-400 mb-4">{t('stats.viewsTrendDesc')}</p>
          <AreaChart
            data={data.viewsSeries}
            labels={data.days.map((d) => d.slice(5))}
            height={240}
            color="#2563EB"
          />
        </Card>

        <Card className="p-5">
          <h3 className="text-base font-semibold mb-4">{t('stats.topArticles')}</h3>
          {data.topArticles.length === 0 ? (
            <p className="text-sm text-default-400">{t('stats.noData')}</p>
          ) : (
            <ul className="space-y-3">
              {data.topArticles.map((a, idx) => (
                <li key={a.slug || idx}>
                  <div className="flex items-center justify-between text-sm">
                    <span className="font-medium truncate">
                      {idx + 1}. {a.title}
                    </span>
                    <span className="text-default-400 ml-2 shrink-0">{a.views}</span>
                  </div>
                  <div className="mt-1 h-1.5 rounded-full bg-default-100 overflow-hidden">
                    <div
                      className="h-full rounded-full bg-primary"
                      style={{ width: `${Math.max(6, (a.views / maxTop) * 100)}%` }}
                    />
                  </div>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>

      {/* 社区经营（订单/收入/积分/会员） */}
      {data.community && (
        <>
          <h3 className="mt-8 mb-1 text-base font-semibold">{t('stats.communityTitle')}</h3>
          <p className="mb-4 text-xs text-default-400">{t('stats.communitySubtitle')}</p>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <StatCard
              title={t('stats.revenueTotal')}
              value={`¥${(data.community.revenueTotal / 100).toFixed(2)}`}
              icon={<MoneyIcon />}
              spark={data.community.revenueSeries}
              sparkColor="#059669"
            />
            <StatCard title={t('stats.ordersPaid')} value={data.community.ordersPaid} icon={<OrderIcon />} />
            <StatCard title={t('stats.ordersPending')} value={data.community.ordersPending} icon={<ClockIcon />} />
            <StatCard title={t('stats.planMembers')} value={data.community.planMembers} icon={<GemIcon />} />
          </div>

          <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-2">
            <Card className="p-5">
              <h3 className="mb-4 text-base font-semibold">{t('stats.revenueTrend')}</h3>
              <AreaChart
                data={data.community.revenueSeries}
                labels={data.days.map((d) => d.slice(5))}
                height={200}
                color="#059669"
              />
              <p className="mt-3 text-xs text-default-400">
                {t('stats.pointsIssued')} {data.community.pointsIssued} · {t('stats.pointsSpent')}{' '}
                {data.community.pointsSpent}
              </p>
            </Card>
            <Card className="p-5">
              <h3 className="mb-4 text-base font-semibold">{t('stats.newMembersTrend')}</h3>
              <AreaChart
                data={data.community.newMembersSeries}
                labels={data.days.map((d) => d.slice(5))}
                height={200}
                color="#2563EB"
              />
            </Card>
          </div>

          {/* 到期提醒：临期/已过期付费会员 */}
          <Card className="mt-6 p-5">
            <h3 className="mb-4 text-base font-semibold">{t('stats.expiringTitle')}</h3>
            {data.community.expiringMembers.length === 0 ? (
              <p className="text-sm text-default-400">{t('stats.expiringEmpty')}</p>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="text-default-400 border-b border-default-200 text-left text-xs">
                      <th className="py-2 pr-4 font-medium">{t('stats.memberCol')}</th>
                      <th className="py-2 pr-4 font-medium">{t('stats.planCol')}</th>
                      <th className="py-2 pr-4 font-medium">{t('stats.expiresCol')}</th>
                      <th className="py-2 font-medium"></th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.community.expiringMembers.map((m, i) => {
                      const expired = new Date(m.planExpiresAt).getTime() < new Date(data.community!.now).getTime()
                      return (
                        <tr key={i} className="border-b border-default-100 last:border-b-0">
                          <td className="py-2 pr-4">
                            <span className="font-medium">{m.name || m.email}</span>
                            {m.name && m.email ? <span className="text-default-400 ml-2 text-xs">{m.email}</span> : null}
                          </td>
                          <td className="py-2 pr-4">{m.plan}</td>
                          <td className="py-2 pr-4 tabular-nums">{m.planExpiresAt.slice(0, 10)}</td>
                          <td className="py-2">
                            <span
                              className={`rounded-full px-2 py-0.5 text-xs font-medium ${
                                expired
                                  ? 'bg-danger-100 text-danger-600'
                                  : 'bg-warning-100 text-warning-600'
                              }`}
                            >
                              {expired ? t('stats.expired') : t('stats.expiring')}
                            </span>
                          </td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </Card>
        </>
      )}
    </PageContainer>
  )
}

export const Route = createFileRoute('/stats')({ component: StatsPage })
