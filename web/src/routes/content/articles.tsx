import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Button } from '@heroui/react'

import { articlesApi, type Article, type FormFieldDef } from '../../api/cms'
import { CONTENT_STATUS_OPTIONS } from '../../components/cms/contentStatus'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { ContentStatusBadge } from '../../components/cms/ContentStatusBadge'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'


const FORM_FIELDS: FormFieldDef[] = [
  { key: 'title', label: '标题', type: 'text', required: true, placeholder: '例如：秋季限定新品上市' },
  {
    key: 'category',
    label: '分类',
    type: 'select',
    options: ['新品动态', '品牌故事', '活动公告', '门店动态'].map((v) => ({ value: v, label: v })),
    defaultValue: '门店动态',
  },
  { key: 'summary', label: '摘要', type: 'textarea', placeholder: '一两句话说明这篇文章讲什么' },
  { key: 'content', label: '正文', type: 'richtext', height: 340, placeholder: '在这里撰写正文，支持加粗、标题、列表，并可插入图片…' },
  { key: 'status', label: '状态', type: 'select', options: CONTENT_STATUS_OPTIONS, defaultValue: 'draft' },
]

function ArticlesPage() {
  const qc = useQueryClient()
  const [editing, setEditing] = useState<Article | null>(null)
  const [modalOpen, setModalOpen] = useState(false)

  const t = useCmsCollection(articlesApi, ['cms-articles'], {
    searchFields: ['title', 'summary', 'category'],
  })

  const openCreate = () => {
    setEditing(null)
    setModalOpen(true)
  }
  const openEdit = (row: Article) => {
    setEditing(row)
    setModalOpen(true)
  }

  const handleSubmit = async (values: Record<string, string | number>) => {
    const contentStr = String(values.content ?? '')
    // 后端 Axum Json 提取器默认 2MB 上限；内联 base64 图片易触发 413。
    // 提交前先估算正文体积（base64 为 ASCII，字符串长度≈字节数），超限给出明确提示而非静默失败。
    if (contentStr.length > 1_500_000) {
      const mb = (contentStr.length / 1024 / 1024).toFixed(2)
      throw new Error(
        `正文体积约 ${mb}MB，已超过后端 2MB 限制，无法保存。请减少图片数量或尺寸，或部署已放宽上限(20MB)的后端。`,
      )
    }
    const patch = {
      title: String(values.title),
      category: String(values.category),
      summary: String(values.summary),
      content: contentStr,
      status: values.status as Article['status'],
    }
    if (editing) await t.update.mutateAsync({ id: editing.id, patch })
    else await t.create.mutateAsync({ ...patch, slug: '', views: 0, author: '我' })
    qc.invalidateQueries({ queryKey: ['home-stats'] })
  }

  const handleDelete = async (row: Article) => {
    if (window.confirm(`确定删除《${row.title}》吗？`)) {
      await t.remove.mutateAsync(row.id)
      qc.invalidateQueries({ queryKey: ['home-stats'] })
    }
  }

  const columns: CmsColumn<Article>[] = [
    {
      id: 'title',
      header: '标题',
      render: (r) => (
        <div className="min-w-0">
          <p className="font-medium text-os-text-primary truncate max-w-[280px]">{r.title}</p>
          <p className="text-xs text-os-text-muted truncate max-w-[280px]">{r.summary || '—'}</p>
        </div>
      ),
    },
    { id: 'category', header: '分类', render: (r) => <span className="text-os-text-secondary">{r.category}</span> },
    { id: 'status', header: '状态', render: (r) => <ContentStatusBadge status={r.status} /> },
    { id: 'views', header: '浏览量', render: (r) => <span className="text-os-text-secondary tabular-nums">{r.views}</span> },
    { id: 'updatedAt', header: '更新时间', render: (r) => <time className="text-xs text-os-text-muted">{fmtDate(r.updatedAt)}</time> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="Articles" desc="文章与动态：写好交给 AI 检查，发布前会请求批准。" />
      <CmsToolbar
        searchPlaceholder="搜索标题 / 摘要 / 分类…"
        searchValue={t.search}
        onSearchChange={t.setSearch}
      >
        <Auth perm={P.contentArticlesCreate}>
          <Button variant="primary" size="sm" onPress={openCreate}>+ 新建文章</Button>
        </Auth>
      </CmsToolbar>

      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="📝"
        emptyTitle="还没有文章"
        emptyHint="点击「新建文章」，或直接让 AI 帮你写一篇"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Auth perm={P.contentArticlesUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => openEdit(row)}>编辑</Button>
            </Auth>
            <Auth perm={P.contentArticlesDelete}>
              <Button
                variant="ghost" size="sm"
                className="text-os-danger-text hover:bg-os-danger-bg"
                onPress={() => handleDelete(row)}
              >
                删除
              </Button>
            </Auth>
          </div>
        )}
      />
      <CmsPagination page={t.page} pageCount={t.pageCount} total={t.total} onPageChange={t.setPage} />

      <CmsFormModal
        title={editing ? `编辑文章：《${editing.title}》` : '新建文章'}
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        onSubmit={handleSubmit}
        fields={FORM_FIELDS}
        initial={editing ?? undefined}
      />
    </div>
  )
}

export const Route = createFileRoute('/content/articles')({
  component: ArticlesPage,
})
