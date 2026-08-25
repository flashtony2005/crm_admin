import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'

import { useState } from 'react'
import { aiTasksApi, type AiTask, type AiTaskStatus, CMS_MODE } from '../../api/cms'
import { request } from '../../api/client'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { fmtRelative } from '../../components/cms/format'
import { StatusBadge, type StatusTone } from '../../components/common/StatusBadge'
import { createFileRoute } from '@tanstack/react-router'

const STATUS_META: Record<AiTaskStatus, { label: string; tone: StatusTone }> = {
  running: { label: '执行中', tone: 'info' },
  waiting_approval: { label: '待批准', tone: 'purple' },
  done: { label: '已完成', tone: 'success' },
  failed: { label: '失败', tone: 'danger' },
}

type Tab = 'tasks' | 'audit'

interface AuditRow {
  id: string
  actor: string
  actorRole: string
  capability: string
  decision: string
  detail: string
  createdAt: string
}

const DECISION_META: Record<string, { label: string; tone: StatusTone }> = {
  executed: { label: '已执行', tone: 'success' },
  escalated: { label: '转审批', tone: 'purple' },
  denied: { label: '已拒绝', tone: 'danger' },
}

function TasksPage() {
  const [tab, setTab] = useState<Tab>('tasks')
  const isReal = CMS_MODE === 'real'
  const { data: tasks = [], isLoading } = useQuery({
    queryKey: ['cms-ai-tasks'],
    queryFn: () => aiTasksApi.list(),
  })

  const { data: auditRows = [] } = useQuery({
    queryKey: ['ai-audit'],
    enabled: isReal && tab === 'audit',
    queryFn: async (): Promise<AuditRow[]> => {
      const body = await request<{ ok: boolean; data: AuditRow[] }>('/api/ai/audit')
      return body.data ?? []
    },
    refetchInterval: 30_000,
  })

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="AI 任务" desc="AI 的每一次执行都有记录，可追溯、可审计。" />

      {isReal && (
        <div className="flex gap-1 mb-4 rounded-lg bg-os-bg-hover p-1 w-fit" role="tablist">
          {([['tasks', '执行记录'], ['audit', '审计日志']] as [Tab, string][]).map(([k, label]) => (
            <button
              key={k}
              role="tab"
              aria-selected={tab === k}
              onClick={() => setTab(k)}
              className={`px-3 h-7 text-sm rounded-md transition-colors ${tab === k ? 'bg-white shadow-sm text-os-text-primary font-medium' : 'text-os-text-muted hover:text-os-text-primary'}`}
            >
              {label}
            </button>
          ))}
        </div>
      )}

      {tab === 'audit' ? (
        <AuditList rows={auditRows} />
      ) : isLoading ? (
        <p className="text-sm text-os-text-muted py-16 text-center">加载中…</p>
      ) : tasks.length === 0 ? (
        <div className="flex flex-col items-center justify-center h-64 text-os-text-muted">
          <div className="text-5xl mb-3 opacity-60">✦</div>
          <p>还没有 AI 任务</p>
          <p className="text-sm mt-1">去首页告诉 AI 你想完成什么</p>
        </div>
      ) : (
        <ul className="flex flex-col gap-2.5">
          {tasks.map((t) => (
            <TaskRow key={t.id} task={t} />
          ))}
        </ul>
      )}
    </div>
  )
}

const AUDIT_TONE = (d: string): StatusTone =>
  d === 'executed' ? 'success' : d === 'escalated' ? 'purple' : 'danger'

function AuditList({ rows }: { rows: AuditRow[] }) {
  if (rows.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-64 text-os-text-muted">
        <p>暂无审计记录</p>
        <p className="text-sm mt-1">AI 的每次尝试（执行 / 转审批 / 拒绝）都会留痕</p>
      </div>
    )
  }
  return (
    <ul className="flex flex-col gap-2">
      {rows.map((a) => {
        const meta = DECISION_META[a.decision] ?? { label: a.decision, tone: 'neutral' as StatusTone }
        return (
          <li key={a.id} className="rounded-xl border border-os-border bg-white px-4 py-3 shadow-sm flex items-center gap-3 flex-wrap">
            <StatusBadge tone={AUDIT_TONE(a.decision)} dot>{meta.label}</StatusBadge>
            <span className="text-xs font-mono px-2 py-0.5 rounded-md bg-gray-100 text-gray-500">{a.capability}</span>
            <span className="text-sm text-os-text-primary flex-1 min-w-0 truncate">{a.detail}</span>
            <span className="text-xs text-os-text-muted">
              @{a.actor}（{a.actorRole}）· {fmtRelative(a.createdAt)}
            </span>
          </li>
        )
      })}
    </ul>
  )
}

function TaskRow({ task }: { task: AiTask }) {
  const meta = STATUS_META[task.status]
  return (
    <li className="rounded-xl border border-os-border bg-white p-4 shadow-sm hover:shadow-md transition-shadow">
      <div className="flex items-start justify-between gap-3 flex-wrap">
        <div className="min-w-0">
          <div className="flex items-center gap-2 mb-1 flex-wrap">
            <StatusBadge tone={meta.tone} dot>{meta.label}</StatusBadge>
            <span className="text-xs px-2 py-0.5 rounded-md bg-gray-100 text-gray-500 font-mono">
              {task.capability}
            </span>
          </div>
          <p className="text-sm font-medium text-os-text-primary">{task.title}</p>
          {task.result && <p className="text-xs text-os-text-muted mt-1">→ {task.result}</p>}
        </div>
        <div className="flex items-center gap-3 flex-shrink-0">
          <time className="text-xs text-os-text-muted">{fmtRelative(task.createdAt)}</time>
          {task.status === 'waiting_approval' && (
            <Link
              to="/ai/approvals"
              className="text-xs px-3 py-1.5 rounded-lg bg-[#6366F1] text-white no-underline hover:bg-[#4F46E5] transition-colors"
            >
              去批准
            </Link>
          )}
        </div>
      </div>
    </li>
  )
}

export const Route = createFileRoute('/ai/tasks')({
  component: TasksPage,
})
