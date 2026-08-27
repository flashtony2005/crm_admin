import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { Button } from '@heroui/react'
import { commentsApi, type Comment } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

const STATUS_LABEL: Record<string, string> = {
  approved: '已通过', pending: '待审核', rejected: '已拒绝', spam: '垃圾',
}

function CommentsPage() {
  const [statusFilter, setStatusFilter] = useState('')
  const t = useCmsCollection(commentsApi, ['cms-comments'], { searchFields: ['authorName', 'content'] })

  const moderate = async (row: Comment, status: string) => {
    await commentsApi.update(row.id, { status } as Partial<Comment>)
    t.refetch()
  }

  const columns: CmsColumn<Comment>[] = [
    { id: 'author', header: '作者', render: (r) => <span className="font-medium">{r.authorName}</span> },
    { id: 'content', header: '内容', render: (r) => <span className="text-sm text-os-text-secondary line-clamp-2 max-w-[360px] block">{r.content}</span> },
    {
      id: 'status', header: '状态', render: (r) =>
        <span className="px-2 py-0.5 rounded-full text-xs bg-gray-100 text-gray-600">{STATUS_LABEL[r.status] ?? r.status}</span>,
    },
    { id: 'createdAt', header: '时间', render: (r) => <time className="text-xs text-os-text-muted">{fmtDate(r.createdAt)}</time> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="评论" desc="内容评论审核。公开站访客可发表评论，管理员在此通过 / 拒绝 / 标记垃圾。" />
      <CmsToolbar searchPlaceholder="搜索昵称 / 内容…" searchValue={t.search} onSearchChange={t.setSearch}>
        <select
          className="text-sm border rounded-md px-2 py-1.5 bg-white"
          value={statusFilter}
          onChange={(e) => setStatusFilter(e.target.value)}
        >
          <option value="">全部状态</option>
          <option value="pending">待审核</option>
          <option value="approved">已通过</option>
          <option value="rejected">已拒绝</option>
          <option value="spam">垃圾</option>
        </select>
      </CmsToolbar>
      <CmsDataTable
        columns={columns}
        rows={statusFilter ? t.paged.filter((r: Comment) => r.status === statusFilter) : t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="💬"
        emptyTitle="还没有评论"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Auth perm={P.contentCommentsUpdate} mode="disable">
              <Button variant="ghost" size="sm" className="text-green-600" onPress={() => void moderate(row, 'approved')}>通过</Button>
            </Auth>
            <Auth perm={P.contentCommentsUpdate} mode="disable">
              <Button variant="ghost" size="sm" className="text-red-500" onPress={() => void moderate(row, 'spam')}>垃圾</Button>
            </Auth>
          </div>
        )}
      />
    </div>
  )
}

export const Route = createFileRoute('/content/comments')({ component: CommentsPage })
