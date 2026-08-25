import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Button } from '@heroui/react'

import { approvalsApi, type Approval, type ApprovalStatus } from '../../api/cms'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { fmtRelative } from '../../components/cms/format'
import { StatusBadge } from '../../components/common/StatusBadge'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'
import { createFileRoute } from '@tanstack/react-router'

const FILTERS: { key: ApprovalStatus | 'all'; label: string }[] = [
  { key: 'pending', label: '待审批' },
  { key: 'approved', label: '已批准' },
  { key: 'rejected', label: '已驳回' },
  { key: 'all', label: '全部' },
]

const ACTION_LABEL: Record<Approval['action'], string> = {
  publish: '发布',
  update: '更新',
  delete: '删除',
}

const RISK_META: Record<Approval['risk'], { label: string; cls: string }> = {
  low: { label: '低风险', cls: 'bg-green-50 text-green-600' },
  mid: { label: '中风险', cls: 'bg-orange-50 text-orange-600' },
  high: { label: '高风险', cls: 'bg-red-50 text-red-600' },
}

function ApprovalsPage() {
  const qc = useQueryClient()
  const [filter, setFilter] = useState<ApprovalStatus | 'all'>('pending')

  const { data: list = [], isLoading } = useQuery({
    queryKey: ['cms-approvals'],
    queryFn: () => approvalsApi.list(),
  })

  // 审批决定（批准 / 驳回）→ 失效缓存，首页统计同步刷新
  const decide = useMutation({
    mutationFn: ({ id, status }: { id: string; status: 'approved' | 'rejected' }) =>
      approvalsApi.decide(id, status),
    onSuccess: () => qc.invalidateQueries(),
  })

  const shown = useMemo(
    () => (filter === 'all' ? list : list.filter((a) => a.status === filter)),
    [list, filter],
  )
  const pendingCount = list.filter((a) => a.status === 'pending').length

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader
        title="审批"
        desc="AI 和团队成员的高风险操作，先经你批准才会执行。"
        actions={
          <span className="text-sm text-os-text-muted">
            待审批 <b className={pendingCount ? 'text-orange-500' : ''}>{pendingCount}</b> 条
          </span>
        }
      />

      {/* 状态过滤 */}
      <div className="flex gap-1.5 mb-4">
        {FILTERS.map((f) => (
          <button
            key={f.key}
            type="button"
            onClick={() => setFilter(f.key)}
            className={`px-3 py-1.5 rounded-lg text-sm transition-colors ${
              filter === f.key
                ? 'bg-[#EEF2FF] text-[#4F46E5] font-medium'
                : 'text-gray-500 hover:bg-gray-100'
            }`}
          >
            {f.label}
          </button>
        ))}
      </div>

      {isLoading ? (
        <p className="text-sm text-os-text-muted py-16 text-center">加载中…</p>
      ) : shown.length === 0 ? (
        <div className="flex flex-col items-center justify-center h-64 text-os-text-muted">
          <div className="text-5xl mb-3 opacity-60">✅</div>
          <p>这里空空的</p>
          <p className="text-sm mt-1">没有符合条件{filter !== 'all' && `「${FILTERS.find((f) => f.key === filter)?.label}」`}的审批</p>
        </div>
      ) : (
        <ul className="flex flex-col gap-3">
          {shown.map((a) => (
            <li key={a.id} className="rounded-xl border border-os-border bg-white p-4 shadow-sm">
              <div className="flex items-center gap-2 mb-1.5 flex-wrap">
                <span className="text-xs px-2 py-0.5 rounded-md bg-indigo-50 text-indigo-600 font-medium">
                  {ACTION_LABEL[a.action]}
                </span>
                <span className={`text-xs px-2 py-0.5 rounded-md font-medium ${RISK_META[a.risk].cls}`}>
                  {RISK_META[a.risk].label}
                </span>
                <StatusBadge tone={a.status === 'approved' ? 'success' : a.status === 'rejected' ? 'danger' : 'purple'} dot>
                  {a.status === 'approved' ? '已批准' : a.status === 'rejected' ? '已驳回' : '待审批'}
                </StatusBadge>
                <time className="ml-auto text-xs text-os-text-muted">{fmtRelative(a.createdAt)}</time>
              </div>

              <p className="text-sm font-medium text-os-text-primary">{a.target}</p>
              <p className="text-xs text-os-text-muted mt-1">{a.summary}</p>

              <div className="flex items-center justify-between mt-3 flex-wrap gap-2">
                <span className="text-xs text-os-text-muted">发起：{a.requestedBy}</span>
                {a.status === 'pending' ? (
                  <div className="flex gap-2">
                    <Auth perm={P.aiApprovalsDecide} mode="hide">
                      <Button
                        size="sm" variant="primary"
                        isDisabled={decide.isPending}
                        onPress={() => decide.mutate({ id: a.id, status: 'approved' })}
                      >
                        批准执行
                      </Button>
                    </Auth>
                    <Auth perm={P.aiApprovalsDecide} mode="hide">
                      <Button
                        size="sm" variant="ghost"
                        className="text-os-danger-text hover:bg-os-danger-bg"
                        isDisabled={decide.isPending}
                        onPress={() => decide.mutate({ id: a.id, status: 'rejected' })}
                      >
                        驳回
                      </Button>
                    </Auth>
                  </div>
                ) : (
                  <span className="text-xs text-os-text-muted">决定时间：{fmtRelative(a.decidedAt)}</span>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

export const Route = createFileRoute('/ai/approvals')({
  component: ApprovalsPage,
})
