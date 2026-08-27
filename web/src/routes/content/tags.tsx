import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Button } from '@heroui/react'

import { tagsApi, type FormFieldDef, type Tag } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

const FORM_FIELDS: FormFieldDef[] = [
  { key: 'name', label: '标签名', type: 'text', required: true, placeholder: '例如：新品' },
  { key: 'slug', label: 'Slug', type: 'text', placeholder: 'URL 别名（留空自动回退标签名）' },
  { key: 'description', label: '描述', type: 'textarea', placeholder: '这个标签代表什么内容' },
  { key: 'coverImage', label: '封面图 URL', type: 'text', placeholder: '标签封面（可留空）' },
  { key: 'metaTitle', label: 'SEO 标题', type: 'text', placeholder: '搜索引擎显示的标题（留空则用标签名）' },
  { key: 'metaDescription', label: 'SEO 描述', type: 'textarea', placeholder: '搜索引擎显示的描述（可留空）' },
]

function TagsPage() {
  const qc = useQueryClient()
  const [editing, setEditing] = useState<Tag | null>(null)
  const [modalOpen, setModalOpen] = useState(false)

  const t = useCmsCollection(tagsApi, ['cms-tags'], {
    searchFields: ['name', 'slug', 'description'],
  })

  const openCreate = () => {
    setEditing(null)
    setModalOpen(true)
  }
  const openEdit = (row: Tag) => {
    setEditing(row)
    setModalOpen(true)
  }

  const handleSubmit = async (values: Record<string, string | number>) => {
    const patch = {
      name: String(values.name),
      slug: String(values.slug ?? '').trim(),
      description: String(values.description ?? ''),
      coverImage: String(values.coverImage ?? '').trim(),
      metaTitle: String(values.metaTitle ?? '').trim(),
      metaDescription: String(values.metaDescription ?? '').trim(),
    }
    if (editing) await t.update.mutateAsync({ id: editing.id, patch })
    else await t.create.mutateAsync(patch)
    qc.invalidateQueries({ queryKey: ['home-stats'] })
  }

  const handleDelete = async (row: Tag) => {
    if (window.confirm(`确定删除标签「${row.name}」吗？（不会删除文章，仅移除该标签条目）`)) {
      await t.remove.mutateAsync(row.id)
    }
  }

  const columns: CmsColumn<Tag>[] = [
    {
      id: 'name',
      header: '标签',
      render: (r) => (
        <div className="min-w-0">
          <p className="font-medium text-os-text-primary truncate max-w-[220px]"># {r.name}</p>
          {r.slug ? <p className="text-xs text-os-text-muted truncate max-w-[220px]">/{r.slug}</p> : null}
        </div>
      ),
    },
    {
      id: 'description',
      header: '描述',
      render: (r) => <span className="text-os-text-secondary text-sm truncate max-w-[320px] block">{r.description || '—'}</span>,
    },
    {
      id: 'coverImage',
      header: '封面',
      render: (r) =>
        r.coverImage ? (
          <img src={r.coverImage} alt="" className="h-9 w-14 object-cover rounded-md" />
        ) : (
          <span className="text-os-text-muted text-xs">—</span>
        ),
    },
    { id: 'updatedAt', header: '更新时间', render: (r) => <time className="text-xs text-os-text-muted">{fmtDate(r.updatedAt)}</time> },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader title="Tags" desc="独立标签库：为标签配置描述、封面与独立 SEO，公开站点击标签可聚合文章。" />
      <CmsToolbar
        searchPlaceholder="搜索标签名 / slug / 描述…"
        searchValue={t.search}
        onSearchChange={t.setSearch}
      >
        <Auth perm={P.contentTagsCreate}>
          <Button variant="primary" size="sm" onPress={openCreate}>+ 新建标签</Button>
        </Auth>
      </CmsToolbar>

      <CmsDataTable
        columns={columns}
        rows={t.paged}
        rowKey={(r) => r.id}
        isLoading={t.isLoading}
        emptyIcon="🏷️"
        emptyTitle="还没有标签"
        emptyHint="新建标签，或在文章编辑页给文章打上标签"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Auth perm={P.contentTagsUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => openEdit(row)}>编辑</Button>
            </Auth>
            <Auth perm={P.contentTagsDelete}>
              <Button
                variant="ghost"
                size="sm"
                className="text-red-500"
                onPress={() => void handleDelete(row)}
              >
                删除
              </Button>
            </Auth>
          </div>
        )}
      />

      <CmsPagination
        page={t.page}
        pageCount={t.pageCount}
        total={t.total}
        onPageChange={t.setPage}
      />

      <CmsFormModal
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        title={editing ? `编辑标签 · ${editing.name}` : '新建标签'}
        fields={FORM_FIELDS}
        initial={editing ?? undefined}
        onSubmit={async (values) => {
          await handleSubmit(values)
          setModalOpen(false)
        }}
      />
    </div>
  )
}

export const Route = createFileRoute('/content/tags')({
  component: TagsPage,
})
