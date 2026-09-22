import { createFileRoute } from '@tanstack/react-router'
import { useEffect, useMemo, useState } from 'react'
import { Card, Spinner } from '@heroui/react'

import { PageContainer } from '../../components/layout/PageContainer'
import { StatCard } from '../../components/common/StatCard'
import { api } from '../../api/client'
import { usePermission } from '../../hooks/usePermission'

/**
 * 分析归因：哪篇文章带来了会员，以及带来了多少钱。
 *
 * 这是本站第一个**只读**分析页：没有任何写操作，因为它回答的是
 * 「过去发生了什么」，而不是「要改什么」。归因结果在注册那一刻就
 * 固化在会员表上（`members.first_touch_article_id`），所以这里只是读出来。
 *
 * 为什么不给时间筛选器：归因的口径是「首次触达」，这是一次性事件 ——
 * 用「最近 30 天」去筛会员，会把 30 天前注册、但最近才付费的人漏掉，
 * 而他的钱是真实进来的。按时间切会让 GMV 对不上账。
 * 要时间维度应该去看统计看板，那里事件是按天落的。
 */

interface AttrRow {
  articleId: string
  title: string
  slug: string
  status: string
  /** 文章已被删除（归因记录还在会员身上） */
  missing: boolean
  reads: number
  visitors: number
  signups: number
  payers: number
  gmvCents: number
}

interface AttrData {
  rows: AttrRow[]
  summary: {
    articles: number
    totalReads: number
    /** 全站去重的访客数 —— 转化率的分母用它 */
    siteVisitors: number
    /** 各文章独立访客之和 —— 同一人跨文章会重复计数，**不能**当分母 */
    sumArticleVisitors: number
    totalMembers: number
    attributedSignups: number
    unattributedSignups: number
    totalPayers: number
    paidOrders: number
    attributedGmvCents: number
    totalGmvCents: number
  }
  gaps: {
    /** 完全没有访客标识：后台建号 / 早期数据 / 未升级前端 */
    noVisitor: number
    /** 有访客标识但注册前没看过任何文章 */
    noRead: number
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
function UserIcon() {
  return (
    <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M16 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2 M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z" />
    </svg>
  )
}
function TargetIcon() {
  return (
    <svg width={20} height={20} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="5" />
      <circle cx="12" cy="12" r="1.6" />
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

function yuan(cents: number): string {
  return `¥${(cents / 100).toFixed(2)}`
}

/** 转化率：注册 / 独立访客。分母为 0 时显示 —，不要显示 0%（那是「没人来」和「来了没转化」的混淆） */
function rate(signups: number, visitors: number): string {
  if (visitors <= 0) return '—'
  return `${((signups / visitors) * 100).toFixed(1)}%`
}

function AttributionPage() {
  const { has } = usePermission()
  const [data, setData] = useState<AttrData | null>(null)
  const [err, setErr] = useState('')

  useEffect(() => {
    let alive = true
    api<AttrData>('/api/admin/attribution')
      .then((d) => alive && setData(d))
      .catch((e) => alive && setErr((e as Error).message))
    return () => {
      alive = false
    }
  }, [])

  // 首屏用来画条形图的基准（最长的一行撑满）
  const maxSignups = useMemo(
    () => Math.max(1, ...(data?.rows ?? []).map((r) => r.signups)),
    [data],
  )

  if (!has('analytics.attribution.view')) {
    return (
      <PageContainer title="分析归因" subtitle="哪篇文章带来了会员与收入">
        <div className="py-8 text-sm text-default-400">没有查看权限，请联系站点所有者。</div>
      </PageContainer>
    )
  }
  if (err) {
    return (
      <PageContainer title="分析归因" subtitle="哪篇文章带来了会员与收入">
        <div className="py-8 text-sm text-danger">{err}</div>
      </PageContainer>
    )
  }
  if (!data) {
    return (
      <div className="grid place-items-center py-16">
        <Spinner />
      </div>
    )
  }

  const { summary, gaps } = data
  const gapTotal = gaps.noVisitor + gaps.noRead

  return (
    <PageContainer title="分析归因" subtitle="哪篇文章带来了会员与收入">
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard title="总阅读" value={summary.totalReads} icon={<EyeIcon />} />
        <StatCard title="独立访客（全站去重）" value={summary.siteVisitors} icon={<UserIcon />} />
        <StatCard title="带来注册" value={summary.attributedSignups} icon={<TargetIcon />} />
        <StatCard title="归因收入" value={yuan(summary.attributedGmvCents)} icon={<MoneyIcon />} />
      </div>

      {/* 归因缺口：必须显式暴露，否则每篇文章的数字都虚高 */}
      {gapTotal > 0 && (
        <Card className="mt-6 border border-warning-200 bg-warning-50 p-4">
          <h3 className="text-sm font-semibold">有 {gapTotal} 位会员无法归因到任何文章</h3>
          <ul className="mt-2 space-y-1 text-xs text-default-600">
            <li>
              · <b>{gaps.noVisitor}</b> 位缺少访客标识 —— 后台手工建号、早期数据，或注册页的埋点未生效。
            </li>
            <li>
              · <b>{gaps.noRead}</b> 位有访客标识但注册前没看过文章 —— 从首页/外部直接注册，属于正常情况。
            </li>
          </ul>
          <p className="mt-2 text-xs text-default-500">
            这些会员<b>不会</b>被摊到任何文章头上。把未归因当成 0 藏起来，会让每一篇文章的战绩都看起来比实际好。
          </p>
        </Card>
      )}

      <Card className="mt-6 p-5">
        <div className="mb-1 flex items-baseline justify-between">
          <h3 className="text-base font-semibold">内容 → 会员 → 收入</h3>
          <span className="text-xs text-default-400">
            共 {summary.articles} 篇有数据 · 归因口径：首次触达
          </span>
        </div>
        <p className="mb-4 text-xs text-default-400">
          首次触达 = 这位会员注册前看过的<b>最早</b>一篇文章。用首次而不是末次，是因为站内推荐位、热门榜、
          「相关阅读」每一次内部跳转都会改写末次触达，最终把转化都归到几个流量入口上，真正的获客入口反而被淹掉。
        </p>

        {data.rows.length === 0 ? (
          <p className="py-6 text-sm text-default-400">
            还没有归因数据。公开站产生阅读并有人注册后，这里就会出现内容。
          </p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-default-200 text-left text-xs text-default-500">
                  <th className="py-2 pr-3 font-medium">文章</th>
                  <th className="py-2 px-3 text-right font-medium">阅读</th>
                  <th className="py-2 px-3 text-right font-medium">独立访客</th>
                  <th className="py-2 px-3 text-right font-medium">带来注册</th>
                  <th className="py-2 px-3 text-right font-medium">转化率</th>
                  <th className="py-2 px-3 text-right font-medium">付费会员</th>
                  <th className="py-2 pl-3 text-right font-medium">归因收入</th>
                </tr>
              </thead>
              <tbody>
                {data.rows.map((r) => (
                  <tr key={r.articleId} className="border-b border-default-100 last:border-0">
                    <td className="py-2.5 pr-3">
                      <div className="max-w-[320px]">
                        <div className="truncate font-medium">
                          {r.missing ? (
                            <span className="text-default-400">（文章已删除）{r.articleId.slice(0, 8)}</span>
                          ) : (
                            r.title || '(无标题)'
                          )}
                        </div>
                        {/* 注册量条形：一眼看出谁是主力入口 */}
                        <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-default-100">
                          <div
                            className="h-full rounded-full bg-primary"
                            style={{ width: `${Math.max(2, (r.signups / maxSignups) * 100)}%` }}
                          />
                        </div>
                      </div>
                    </td>
                    <td className="px-3 py-2.5 text-right tabular-nums text-default-600">{r.reads}</td>
                    <td className="px-3 py-2.5 text-right tabular-nums text-default-600">{r.visitors}</td>
                    <td className="px-3 py-2.5 text-right font-semibold tabular-nums">
                      {r.signups > 0 ? r.signups : <span className="font-normal text-default-300">0</span>}
                    </td>
                    <td className="px-3 py-2.5 text-right tabular-nums text-default-500">
                      {rate(r.signups, r.visitors)}
                    </td>
                    <td className="px-3 py-2.5 text-right tabular-nums text-default-600">
                      {r.payers > 0 ? r.payers : <span className="text-default-300">0</span>}
                    </td>
                    <td className="py-2.5 pl-3 text-right tabular-nums">
                      {r.gmvCents > 0 ? (
                        <span className="font-medium text-success-600">{yuan(r.gmvCents)}</span>
                      ) : (
                        <span className="text-default-300">—</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr className="text-xs text-default-500">
                  <td className="pt-3 pr-3">合计</td>
                  <td className="px-3 pt-3 text-right tabular-nums">{summary.totalReads}</td>
                  <td className="px-3 pt-3 text-right tabular-nums">{summary.sumArticleVisitors}</td>
                  <td className="px-3 pt-3 text-right tabular-nums">{summary.attributedSignups}</td>
                  <td className="px-3 pt-3 text-right">
                    {rate(summary.attributedSignups, summary.siteVisitors)}
                  </td>
                  <td className="px-3 pt-3 text-right tabular-nums">{summary.totalPayers}</td>
                  <td className="py-3 pl-3 text-right tabular-nums">{yuan(summary.attributedGmvCents)}</td>
                </tr>
              </tfoot>
            </table>
          </div>
        )}
        <p className="mt-3 text-xs text-default-400">
          合计行的「独立访客」是各篇文章相加，同一个人跨多篇阅读会被重复计入，因此它
          <b>大于等于</b>顶部卡片的「独立访客（全站去重）」；两者口径不同，都保留是有意的。
        </p>
      </Card>

      {/* 全站口径对账 */}
      <Card className="mt-6 p-5">
        <h3 className="mb-3 text-base font-semibold">对账</h3>
        <div className="grid grid-cols-1 gap-x-8 gap-y-2 text-sm sm:grid-cols-2">
          <div className="flex justify-between border-b border-default-100 py-1.5">
            <span className="text-default-500">全站会员总数</span>
            <span className="tabular-nums">{summary.totalMembers}</span>
          </div>
          <div className="flex justify-between border-b border-default-100 py-1.5">
            <span className="text-default-500">其中已归因到文章</span>
            <span className="tabular-nums">
              {summary.attributedSignups}
              <span className="ml-2 text-xs text-default-400">
                {summary.totalMembers > 0
                  ? `${((summary.attributedSignups / summary.totalMembers) * 100).toFixed(0)}%`
                  : '—'}
              </span>
            </span>
          </div>
          <div className="flex justify-between border-b border-default-100 py-1.5">
            <span className="text-default-500">全站已支付订单</span>
            <span className="tabular-nums">{summary.paidOrders}</span>
          </div>
          <div className="flex justify-between border-b border-default-100 py-1.5">
            <span className="text-default-500">全站已支付总额</span>
            <span className="tabular-nums">{yuan(summary.totalGmvCents)}</span>
          </div>
          <div className="flex justify-between border-b border-default-100 py-1.5">
            <span className="text-default-500">其中可归因到文章</span>
            <span className="tabular-nums">
              {yuan(summary.attributedGmvCents)}
              <span className="ml-2 text-xs text-default-400">
                {summary.totalGmvCents > 0
                  ? `${((summary.attributedGmvCents / summary.totalGmvCents) * 100).toFixed(0)}%`
                  : '—'}
              </span>
            </span>
          </div>
        </div>
        <p className="mt-3 text-xs text-default-400">
          已有会员的付费不会追溯改写归因 —— 会员是在注册那一刻绑定的首次触达。所以「可归因」占比会随着
          老会员续费而低于阅读占比，这是正常的，不是漏数。
        </p>
      </Card>
    </PageContainer>
  )
}

export const Route = createFileRoute('/content/attribution')({
  component: AttributionPage,
})
