import { Link } from '@tanstack/react-router'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button } from '@heroui/react'

import { approvalsApi, getHomeStats } from '../api/cms'
import { AiPromptCard } from '../components/cms/AiPromptCard'
import { StatusBadge, toneFromStatus } from '../components/common/StatusBadge'
import { createFileRoute } from '@tanstack/react-router'

/** 首页数据 hook：聚合统计 + 待审批列表 */
function useHomeData() {
  const qc = useQueryClient()
  const stats = useQuery({ queryKey: ['home-stats'], queryFn: getHomeStats })
  const decide = async (id: string, status: 'approved' | 'rejected') => {
    await approvalsApi.decide(id, status)
    qc.invalidateQueries()
  }
  return { stats: stats.data, isLoading: stats.isLoading, decide }
}

function HomePage() {
  const { stats, isLoading, decide } = useHomeData()

  const cards = [
    { label: '待你审批', value: stats?.pendingApprovals.length ?? 0, to: '/ai/approvals', accent: '#F97316', bg: '#FFF7ED' },
    { label: '高优先级客户', value: stats?.highPriorityCustomers.length ?? 0, to: '/business/customers', accent: '#6366F1', bg: '#EEF2FF' },
    { label: '新线索', value: stats?.newLeads.length ?? 0, to: '/business/leads', accent: '#22C55E', bg: '#F0FDF4' },
    { label: 'AI 进行中任务', value: stats?.runningTasks.length ?? 0, to: '/ai/tasks', accent: '#8B5CF6', bg: '#F5F3FF' },
  ]

  return (
    <div className="p-1 md:p-2 flex flex-col gap-5">
      {/* 欢迎语 + AI 主入口 */}
      <section>
        <h1 className="text-2xl font-semibold text-os-text-primary tracking-tight mb-1">
          今天想完成什么？
        </h1>
        <p className="text-sm text-os-text-muted mb-4">
          用一句话把事情交给 AI，重要操作会先请求你的批准。
        </p>
        <AiPromptCard />
      </section>

      {/* 统计卡 */}
      <section className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        {cards.map((c) => (
          <Link key={c.label} to={c.to} className="no-underline">
            <div className="rounded-xl border border-os-border bg-white p-4 shadow-sm hover:shadow-md hover:-translate-y-0.5 transition-all cursor-pointer h-full">
              <div className="w-9 h-9 rounded-lg flex items-center justify-center text-lg font-semibold" style={{ background: c.bg, color: c.accent }}>
                {c.value}
              </div>
              <p className="mt-3 text-sm text-os-text-secondary">{c.label}</p>
            </div>
          </Link>
        ))}
      </section>

      <div className="grid lg:grid-cols-2 gap-5">
        {/* 待审批 */}
        <section className="rounded-xl border border-os-border bg-white shadow-sm p-4">
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-base font-semibold text-os-text-primary">等待审批</h2>
            <Link to="/ai/approvals" className="text-xs text-[#6366F1] no-underline hover:underline">全部 →</Link>
          </div>
          {isLoading ? (
            <p className="text-sm text-os-text-muted py-6 text-center">加载中…</p>
          ) : (stats?.pendingApprovals.length ?? 0) === 0 ? (
            <p className="text-sm text-os-text-muted py-6 text-center">🎉 没有待审批的操作</p>
          ) : (
            <ul className="flex flex-col gap-2.5">
              {stats!.pendingApprovals.map((a) => (
                <li key={a.id} className="rounded-lg border border-os-border-light p-3">
                  <div className="flex items-center gap-2 mb-1 flex-wrap">
                    <StatusBadge tone={toneFromStatus(a.risk === 'high' ? 'warning' : 'info')} dot>
                      {a.action === 'publish' ? '发布' : a.action === 'update' ? '更新' : '删除'}
                    </StatusBadge>
                    <span className="text-sm font-medium text-os-text-primary truncate">{a.target}</span>
                  </div>
                  <p className="text-xs text-os-text-muted mb-2">{a.summary}</p>
                  <div className="flex gap-2">
                    <Button size="sm" variant="primary" onPress={() => decide(a.id, 'approved')}>批准</Button>
                    <Button size="sm" variant="ghost" onPress={() => decide(a.id, 'rejected')}>驳回</Button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>

        {/* AI 任务动态 */}
        <section className="rounded-xl border border-os-border bg-white shadow-sm p-4">
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-base font-semibold text-os-text-primary">AI 最近在做什么</h2>
            <Link to="/ai/tasks" className="text-xs text-[#6366F1] no-underline hover:underline">全部 →</Link>
          </div>
          {isLoading ? (
            <p className="text-sm text-os-text-muted py-6 text-center">加载中…</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {(stats?.runningTasks ?? []).map((t) => (
                <li key={t.id} className="flex items-center gap-2.5 rounded-lg px-3 py-2.5 hover:bg-os-bg-hover transition-colors">
                  <StatusBadge tone={t.status === 'running' ? 'info' : 'purple'} dot>
                    {t.status === 'running' ? '执行中' : '待批准'}
                  </StatusBadge>
                  <span className="text-sm text-os-text-primary truncate">{t.title}</span>
                </li>
              ))}
              {(stats?.runningTasks.length ?? 0) === 0 && (
                <li className="text-sm text-os-text-muted py-6 text-center">AI 暂无进行中的任务</li>
              )}
            </ul>
          )}
        </section>
      </div>
    </div>
  )
}

export const Route = createFileRoute('/home')({
  component: HomePage,
})
