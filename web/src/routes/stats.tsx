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

function StatsPage() {
  const { t } = useTranslation()
  const { has } = usePermission()
  const [data, setData] = useState<StatsData | null>(null)
  const [err, setErr] = useState('')

  useEffect(() => {
    let alive = true
    api<StatsData>('/api/admin/stats')
      .then((d) => alive && setData(d))
      .catch((e) => alive && setErr((e as Error).message))
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
  if (err) {
    return (
      <PageContainer title={t('stats.title')} subtitle={t('stats.subtitle')}>
        <div className="text-danger text-sm py-8">{err}</div>
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
    </PageContainer>
  )
}

export const Route = createFileRoute('/stats')({ component: StatsPage })
