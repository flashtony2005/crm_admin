import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { createFileRoute } from '@tanstack/react-router'

import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsPagination } from '../../components/cms/CmsToolbar'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'
import { fmtRelative } from '../../components/cms/format'
import { auditApi } from '../../api/cms'
import type { AuditRow } from '../../api/cms/types'

function methodTone(m: string): string {
  if (m === 'DELETE') return 'bg-rose-50 text-rose-600'
  if (m === 'POST') return 'bg-emerald-50 text-emerald-600'
  if (m === 'PUT' || m === 'PATCH') return 'bg-blue-50 text-blue-600'
  return 'bg-gray-100 text-gray-600'
}

function statusTone(s: number): string {
  if (s < 400) return 'bg-emerald-50 text-emerald-600'
  if (s < 500) return 'bg-amber-50 text-amber-600'
  return 'bg-rose-50 text-rose-600'
}

function AuditLogPage() {
  const [page, setPage] = useState(1)
  const pageSize = 50
  const q = useQuery({
    queryKey: ['admin-audit', page, pageSize],
    queryFn: () => auditApi.list(page, pageSize),
  })
  const rows = q.data?.items ?? []
  const total = q.data?.total ?? 0
  const pageCount = Math.max(1, Math.ceil(total / pageSize))

  const columns: CmsColumn<AuditRow>[] = [
    { id: 'createdAt', header: '时间', render: (r) => <time className="text-xs text-os-text-muted">{fmtRelative(r.createdAt)}</time> },
    { id: 'username', header: '操作者', render: (r) => <span className="font-medium">{r.username || '匿名'}</span> },
    {
      id: 'method', header: '方法', render: (r) => (
        <span className={`px-2 py-0.5 rounded text-xs font-mono ${methodTone(r.method)}`}>{r.method}</span>
      ),
    },
    { id: 'path', header: '路径', render: (r) => <code className="text-xs">{r.path}</code> },
    {
      id: 'status', header: '状态码', render: (r) => (
        <span className={`px-2 py-0.5 rounded-full text-xs ${statusTone(r.status)}`}>{r.status}</span>
      ),
    },
    { id: 'durationMs', header: '耗时', render: (r) => <span className="text-xs text-os-text-muted">{r.durationMs} ms</span> },
    { id: 'requestId', header: 'Request ID', render: (r) => <code className="text-xs text-os-text-muted">{r.requestId.slice(0, 8)}</code> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader
        title="审计日志"
        desc="全站写操作留痕：登录 / 注册 / 下单 / 内容改动等。每条记录含结果状态与耗时，Request ID 可与网关日志串联追踪。"
      />
      <CmsDataTable
        columns={columns}
        rows={rows}
        rowKey={(r) => `${r.requestId}-${r.createdAt}`}
        isLoading={q.isLoading}
        emptyIcon="🧾"
        emptyTitle="暂无审计记录"
        emptyHint="有写操作（POST / PUT / PATCH / DELETE）后这里会出现记录"
      />
      <CmsPagination page={page} pageCount={pageCount} total={total} onPageChange={setPage} />
    </div>
  )
}

export const Route = createFileRoute('/team/audit')({
  component: () => (
    <Auth perm={P.teamUsersUpdate}>
      <AuditLogPage />
    </Auth>
  ),
})
