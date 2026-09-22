import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { Button } from '@heroui/react'

import {
  notFoundApi,
  redirectsApi,
  type FormFieldDef,
  type NotFoundLog,
  type Redirect,
} from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

/** 状态码可选项 —— 只列真的会用的五个，不做「HTTP 状态码大全」 */
const CODE_OPTIONS = [
  { value: '301', label: '301 · 永久迁移（推荐：权重完整传递）' },
  { value: '302', label: '302 · 临时跳转' },
  { value: '307', label: '307 · 临时（保持请求方法）' },
  { value: '308', label: '308 · 永久（保持请求方法）' },
  { value: '410', label: '410 · 内容已永久移除（告诉爬虫别再来）' },
]

const CODE_LABEL: Record<string, string> = {
  '301': '301 永久',
  '302': '302 临时',
  '307': '307 临时',
  '308': '308 永久',
  '410': '410 已移除',
}

const REDIRECT_FIELDS: FormFieldDef[] = [
  {
    key: 'fromPath',
    label: '来源路径',
    type: 'text',
    required: true,
    placeholder: '/t/coucouya/post/old-slug（含模板前缀，与 404 日志一致）',
  },
  {
    key: 'toPath',
    label: '目标地址',
    type: 'text',
    required: true,
    placeholder: '/post/new-slug，或 https://…（410 可不填）',
  },
  { key: 'code', label: '状态码', type: 'select', options: CODE_OPTIONS, defaultValue: 301 },
  { key: 'note', label: '备注', type: 'text', placeholder: '为什么加这条（如：slug 变更 / 内容下线）' },
]

function RedirectsPage() {
  const [tab, setTab] = useState<'redirects' | 'notfound'>('redirects')
  const [modalOpen, setModalOpen] = useState(false)
  const [editing, setEditing] = useState<Redirect | null>(null)
  /** 从 404 日志「一键转 301」进来时，记下那条日志 —— 提交后顺手标为已处理 */
  const [convertFrom, setConvertFrom] = useState<NotFoundLog | null>(null)

  const r = useCmsCollection(redirectsApi, ['cms-redirects'], {
    searchFields: ['fromPath', 'toPath', 'note'],
  })
  const n = useCmsCollection(notFoundApi, ['cms-notfound'], {
    searchFields: ['path', 'referer', 'ua'],
  })

  const openCreate = () => {
    setEditing(null)
    setConvertFrom(null)
    setModalOpen(true)
  }
  const openEdit = (row: Redirect) => {
    setEditing(row)
    setConvertFrom(null)
    setModalOpen(true)
  }
  const openConvert = (row: NotFoundLog) => {
    setEditing(null)
    setConvertFrom(row)
    setModalOpen(true)
  }

  const handleSubmit = async (values: Record<string, string | number>) => {
    const patch = {
      fromPath: String(values.fromPath ?? '').trim(),
      toPath: String(values.toPath ?? '').trim(),
      code: Number(values.code ?? 301),
      note: String(values.note ?? '').trim(),
    }
    if (editing) {
      await r.update.mutateAsync({ id: editing.id, patch })
    } else {
      // 同 links：hits 由服务端累加，但实体契约要求显式初值。
      await r.create.mutateAsync({ ...patch, enabled: true, hits: 0 })
      // 从 404 日志转过来的，顺手把那条标为已处理 ——
      // 否则已解决条目会一直在列表里堆积，真正该看的被埋掉
      if (convertFrom) {
        await n.update.mutateAsync({ id: convertFrom.id, patch: { resolved: true } })
      }
    }
    setModalOpen(false)
  }

  const markResolved = async (row: NotFoundLog) => {
    await n.update.mutateAsync({ id: row.id, patch: { resolved: true } })
  }

  const redirectColumns: CmsColumn<Redirect>[] = [
    {
      id: 'fromPath',
      header: '来源 → 目标',
      render: (row) => (
        <div className="min-w-0">
          <p className="font-medium text-os-text-primary truncate max-w-[320px]">{row.fromPath}</p>
          <p className="text-xs text-os-text-muted truncate max-w-[320px]">→ {row.toPath || '（无，410 移除）'}</p>
        </div>
      ),
    },
    {
      id: 'code',
      header: '状态码',
      render: (row) => (
        <span className="text-xs px-2 py-0.5 rounded-full bg-os-bg-hover text-os-text-secondary">
          {CODE_LABEL[String(row.code)] ?? row.code}
        </span>
      ),
    },
    {
      id: 'hits',
      header: '命中',
      render: (row) => (
        <span className={row.hits > 0 ? 'font-medium text-os-text-primary' : 'text-os-text-muted'}>
          {row.hits}
        </span>
      ),
    },
    {
      id: 'enabled',
      header: '状态',
      render: (row) => (
        <span className={row.enabled ? 'text-emerald-600 text-xs' : 'text-os-text-muted text-xs'}>
          {row.enabled ? '生效中' : '已停用'}
        </span>
      ),
    },
    {
      id: 'note',
      header: '备注',
      render: (row) => (
        <span className="text-os-text-secondary text-sm truncate max-w-[200px] block">{row.note || '—'}</span>
      ),
    },
    {
      id: 'updatedAt',
      header: '更新时间',
      render: (row) => <time className="text-xs text-os-text-muted">{fmtDate(row.updatedAt)}</time>,
    },
  ]

  const notFoundColumns: CmsColumn<NotFoundLog>[] = [
    {
      id: 'path',
      header: '未找到的地址',
      render: (row) => (
        <div className="min-w-0">
          <p className="font-medium text-os-text-primary truncate max-w-[320px]">{row.path}</p>
          {row.referer ? (
            <p className="text-xs text-os-text-muted truncate max-w-[320px]">来源页 {row.referer}</p>
          ) : null}
        </div>
      ),
    },
    {
      id: 'hits',
      header: '次数',
      render: (row) => <span className="font-medium text-os-text-primary">{row.hits}</span>,
    },
    {
      id: 'resolved',
      header: '处理',
      render: (row) => (
        <span className={row.resolved ? 'text-emerald-600 text-xs' : 'text-amber-600 text-xs'}>
          {row.resolved ? '已处理' : '待处理'}
        </span>
      ),
    },
    {
      id: 'ua',
      header: '客户端',
      render: (row) => (
        <span className="text-os-text-muted text-xs truncate max-w-[220px] block">{row.ua || '—'}</span>
      ),
    },
    {
      id: 'lastSeen',
      header: '最近出现',
      render: (row) => <time className="text-xs text-os-text-muted">{fmtDate(row.lastSeen)}</time>,
    },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader
        title="重定向"
        desc="URL 迁移与 404 监控：改 slug、换类型、下线内容时，别让旧地址静默流失权重。301 在服务端生效，爬虫同样跟随。"
      />

      <div className="flex gap-2 mb-3">
        <Button
          variant={tab === 'redirects' ? 'primary' : 'ghost'}
          size="sm"
          onPress={() => setTab('redirects')}
        >
          重定向规则（{r.total}）
        </Button>
        <Button
          variant={tab === 'notfound' ? 'primary' : 'ghost'}
          size="sm"
          onPress={() => setTab('notfound')}
        >
          404 监控（{n.total}）
        </Button>
      </div>

      {tab === 'redirects' ? (
        <>
          <CmsToolbar
            searchPlaceholder="搜索来源 / 目标 / 备注…"
            searchValue={r.search}
            onSearchChange={r.setSearch}
          >
            <Auth perm={P.contentSeoCreate}>
              <Button variant="primary" size="sm" onPress={openCreate}>
                + 新建重定向
              </Button>
            </Auth>
          </CmsToolbar>

          <CmsDataTable
            columns={redirectColumns}
            rows={r.paged}
            rowKey={(row) => row.id}
            isLoading={r.isLoading}
            emptyIcon="↪️"
            emptyTitle="还没有重定向规则"
            emptyHint="改过 slug 或下线过内容？在 404 监控里看到的具体死链，可以一键转成 301"
            actions={(row) => (
              <div className="flex gap-1.5">
                <Auth perm={P.contentSeoUpdate} mode="disable">
                  <Button variant="ghost" size="sm" onPress={() => openEdit(row)}>
                    编辑
                  </Button>
                </Auth>
                <Auth perm={P.contentSeoDelete}>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-red-500"
                    onPress={() => {
                      if (window.confirm(`确定删除「${row.fromPath}」这条重定向吗？`)) {
                        void r.remove.mutateAsync(row.id)
                      }
                    }}
                  >
                    删除
                  </Button>
                </Auth>
              </div>
            )}
          />

          <CmsPagination page={r.page} pageCount={r.pageCount} total={r.total} onPageChange={r.setPage} />
        </>
      ) : (
        <>
          <CmsToolbar
            searchPlaceholder="搜索地址 / 来源页 / 客户端…"
            searchValue={n.search}
            onSearchChange={n.setSearch}
          />

          <CmsDataTable
            columns={notFoundColumns}
            rows={n.paged}
            rowKey={(row) => row.id}
            isLoading={n.isLoading}
            emptyIcon="✅"
            emptyTitle="没有死链记录"
            emptyHint="公开站渲染「未找到」时会自动上报；有记录说明有链接指向了不存在的页面"
            actions={(row) => (
              <div className="flex gap-1.5">
                <Auth perm={P.contentSeoCreate} mode="disable">
                  <Button variant="ghost" size="sm" onPress={() => openConvert(row)}>
                    转 301
                  </Button>
                </Auth>
                {!row.resolved ? (
                  <Auth perm={P.contentSeoUpdate} mode="disable">
                    <Button variant="ghost" size="sm" onPress={() => void markResolved(row)}>
                      标为已处理
                    </Button>
                  </Auth>
                ) : null}
              </div>
            )}
          />

          <CmsPagination page={n.page} pageCount={n.pageCount} total={n.total} onPageChange={n.setPage} />
        </>
      )}

      <CmsFormModal
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        title={editing ? `编辑重定向 · ${editing.fromPath}` : '新建重定向'}
        fields={REDIRECT_FIELDS}
        initial={
          editing
            ? { ...editing, code: String(editing.code) }
            : convertFrom
              ? { fromPath: convertFrom.path, toPath: '', code: '301', note: '由 404 监控生成' }
              : undefined
        }
        onSubmit={async (values) => {
          await handleSubmit(values)
        }}
      />
    </div>
  )
}

export const Route = createFileRoute('/content/redirects')({
  component: RedirectsPage,
})
