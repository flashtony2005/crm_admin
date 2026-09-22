import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { Button } from '@heroui/react'

import { linksApi, type FormFieldDef, type SmartLink } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { CmsPagination, CmsToolbar } from '../../components/cms/CmsToolbar'
import { useCmsCollection } from '../../components/cms/useCmsCollection'
import { fmtDate } from '../../components/cms/format'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

/**
 * 短链（P0-4）。对标 FluentCRM 的 link tracking / Bitly，但**不引第三方**。
 *
 * 存在的理由：私有化站点没有 Google Analytics，也没有第三方像素。
 * 「我发出去的那条链接，到底有没有人点、点的都是谁」这个问题，
 * 只能靠自己伺服一个 `/go/{token}` 来回答。它是本站**唯一**能拿到的
 * 用户行为信号，也是往后做分群、做再触达的数据起点。
 *
 * 两个数字必须分开显示（`clicks` / `prefetch`）：
 * 邮件网关和 IM 会先把链接抓一遍做预览。把预取混进点击数，
 * 一封发给 200 人的邮件在无人点开时就能显示「200 次点击」——
 * 这个数字一旦被信过，后面所有判断都是错的。
 */

/**
 * 短链对外的前缀。
 *
 * 短链是要**贴到邮件、IM、短信**里的，必须是外部世界能直接打开的绝对地址。
 * 后台自己的 origin（`http://localhost:5188`）对外没有意义，
 * 所以优先用 `VITE_PUBLIC_BASE_URL`（生产域名）；没配才回落到当前 origin
 * （本地联调够用，页面顶部会给出提示）。
 */
const LINK_BASE: string =
  (import.meta.env.VITE_PUBLIC_BASE_URL as string | undefined)?.replace(/\/+$/, '') ||
  (typeof window !== 'undefined' ? window.location.origin : '')

function shortUrl(token: string): string {
  return token ? `${LINK_BASE}/go/${token}` : ''
}

/** 随机 token：短链通常靠复制传播而非手打，所以不需要讨好输入法。 */
function genToken(): string {
  const a = 'abcdefghijkmnpqrstuvwxyz23456789'
  let out = ''
  for (let i = 0; i < 7; i += 1) out += a[Math.floor(Math.random() * a.length)]
  return out
}

const LINK_FIELDS: FormFieldDef[] = [
  {
    key: 'token',
    label: '短链尾段',
    type: 'text',
    required: true,
    placeholder: '如 spring24（只能字母/数字/-/_，决定 /go/xxx）',
  },
  {
    key: 'url',
    label: '目标地址',
    type: 'text',
    required: true,
    placeholder: 'https://…（只允许 http/https）',
  },
  { key: 'label', label: '备注名', type: 'text', placeholder: '如：3月新品推广 · 微信群' },
  {
    key: 'tags',
    label: '点击后自动打的标签',
    type: 'text',
    placeholder: '逗号分隔，如 vip,高意向（点开链接的人会自动获得）',
  },
  {
    key: 'enabled',
    label: '状态',
    type: 'select',
    options: [
      { value: '1', label: '启用' },
      { value: '0', label: '停用（保留统计，不跳转）' },
    ],
    defaultValue: '1',
  },
]

function LinksPage() {
  const [modalOpen, setModalOpen] = useState(false)
  const [editing, setEditing] = useState<SmartLink | null>(null)

  const c = useCmsCollection(linksApi, ['cms-links'], {
    searchFields: ['token', 'url', 'label', 'tags'],
  })

  const totalClicks = c.items.reduce((n, r) => n + (r.clicks || 0), 0)
  const totalPrefetch = c.items.reduce((n, r) => n + (r.prefetch || 0), 0)

  const openCreate = () => {
    setEditing(null)
    setModalOpen(true)
  }
  const openEdit = (row: SmartLink) => {
    setEditing(row)
    setModalOpen(true)
  }

  const handleSubmit = async (values: Record<string, string | number>) => {
    const patch = {
      token: String(values.token ?? '').trim().toLowerCase(),
      url: String(values.url ?? '').trim(),
      label: String(values.label ?? '').trim(),
      tags: String(values.tags ?? '').trim(),
      enabled: Number(values.enabled ?? 1) === 1,
    }
    if (editing) {
      await c.update.mutateAsync({ id: editing.id, patch })
    } else {
      // 计数器归服务端所有，但 create 的契约是「完整实体」——
      // 显式给初值而不是省字段：省掉会在编译期报缺字段，
      // 硬塞 undefined 又会让「谁在写这个计数」变得含糊。
      await c.create.mutateAsync({ ...patch, clicks: 0, prefetch: 0, lastClickAt: null })
    }
    setModalOpen(false)
  }

  const copy = async (token: string) => {
    const url = shortUrl(token)
    try {
      await navigator.clipboard.writeText(url)
    } catch {
      // 非 HTTPS 或权限被拒时 clipboard 不可用 —— 退化成让用户自己复制
      window.prompt('复制这条短链：', url)
    }
  }

  const columns: CmsColumn<SmartLink>[] = [
    {
      id: 'link',
      header: '短链',
      render: (row) => (
        <div className="min-w-0">
          <p className="font-medium text-os-text-primary truncate max-w-[340px]">
            {row.label || row.token}
          </p>
          <p className="text-xs text-os-text-muted truncate max-w-[340px]">{shortUrl(row.token)}</p>
        </div>
      ),
    },
    {
      id: 'url',
      header: '目标',
      render: (row) =>
        // 只有 http(s) 才渲染成链接。后端已经拒绝把 javascript: 用作跳转目标，
        // 但**后台自己的列表**如果把它渲染成 <a href>，点一下仍然会在后台执行 ——
        // 跳转防护救不了这个面。所以这里再挡一道。
        /^https?:\/\//i.test(row.url) ? (
          <a
            href={row.url}
            target="_blank"
            rel="noreferrer"
            className="text-xs text-os-text-muted truncate max-w-[260px] block hover:underline"
          >
            {row.url}
          </a>
        ) : (
          <span className="text-xs text-red-500 truncate max-w-[260px] block" title="非法目标地址（只允许 http/https）">
            {row.url || '（空）'} · 非法
          </span>
        ),
    },
    {
      id: 'tags',
      header: '自动标签',
      render: (row) =>
        row.tags ? (
          <div className="flex flex-wrap gap-1">
            {row.tags
              .split(/[,，]/)
              .map((t) => t.trim())
              .filter(Boolean)
              .map((t) => (
                <span
                  key={t}
                  className="text-xs px-1.5 py-0.5 rounded bg-os-bg-hover text-os-text-muted"
                >
                  {t}
                </span>
              ))}
          </div>
        ) : (
          <span className="text-xs text-os-text-muted">—</span>
        ),
    },
    {
      id: 'clicks',
      header: '真人点击',
      render: (row) => (
        <div className="min-w-0">
          <p className="font-medium text-os-text-primary">{row.clicks}</p>
          {/* 预取单独标注：这个数字大不代表链接火了，只代表邮件被网关扫过 */}
          <p className="text-xs text-os-text-muted">预取 {row.prefetch}</p>
        </div>
      ),
    },
    {
      id: 'lastClickAt',
      header: '最近点击',
      render: (row) => (
        <time className="text-xs text-os-text-muted">
          {fmtDate(row.lastClickAt ?? undefined)}
        </time>
      ),
    },
    {
      id: 'enabled',
      header: '状态',
      render: (row) => (
        <span className={row.enabled ? 'text-emerald-600 text-xs' : 'text-os-text-muted text-xs'}>
          {row.enabled ? '启用' : '停用'}
        </span>
      ),
    },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader
        title="短链"
        desc="对外发 /go/xxx 代替裸链接：谁点了、点了几个、点的人自动打上什么标签，全部自己留档。"
      />

      {!import.meta.env.VITE_PUBLIC_BASE_URL && (
        <div className="mb-3 rounded-lg border border-amber-300/50 bg-amber-50 dark:bg-amber-950/20 px-3 py-2 text-xs text-os-text-muted">
          当前生成的短链用的是后台自身地址（{LINK_BASE}），对外发出去打不开。
          生产环境请设置 <code>VITE_PUBLIC_BASE_URL</code> 指向站点公开域名。
        </div>
      )}

      <div className="mb-3 flex flex-wrap gap-4 text-xs text-os-text-muted">
        <span>
          链接 <strong className="text-os-text-primary">{c.total}</strong> 条
        </span>
        <span>
          真人点击 <strong className="text-os-text-primary">{totalClicks}</strong>
        </span>
        <span>
          预取（机器人）<strong className="text-os-text-primary">{totalPrefetch}</strong>
        </span>
      </div>

      <CmsToolbar
        searchPlaceholder="搜索短链 / 目标 / 备注 / 标签…"
        searchValue={c.search}
        onSearchChange={c.setSearch}
      >
        <Auth perm={P.contentSeoCreate}>
          <Button variant="primary" size="sm" onPress={openCreate}>
            + 新建短链
          </Button>
        </Auth>
      </CmsToolbar>

      <CmsDataTable
        columns={columns}
        rows={c.paged}
        rowKey={(row) => row.id}
        isLoading={c.isLoading}
        emptyIcon="🔗"
        emptyTitle="还没有短链"
        emptyHint="建一条短链，发在邮件或群里 —— 之后就能看到「谁点了、点了几个」，而不是只有一个空白"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Button variant="ghost" size="sm" onPress={() => void copy(row.token)}>
              复制
            </Button>
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
                  if (
                    window.confirm(
                      `确定删除「${row.label || row.token}」吗？已累计 ${row.clicks} 次点击的记录会一起消失。`,
                    )
                  ) {
                    void c.remove.mutateAsync(row.id)
                  }
                }}
              >
                删除
              </Button>
            </Auth>
          </div>
        )}
      />

      <CmsPagination page={c.page} pageCount={c.pageCount} total={c.total} onPageChange={c.setPage} />

      <CmsFormModal
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        title={editing ? `编辑短链 · ${editing.token}` : '新建短链'}
        fields={LINK_FIELDS}
        initial={
          editing
            ? { ...editing, enabled: editing.enabled ? '1' : '0' }
            : { token: genToken(), enabled: '1' }
        }
        onSubmit={async (values) => {
          await handleSubmit(values)
        }}
      />
    </div>
  )
}

export const Route = createFileRoute('/content/links')({
  component: LinksPage,
})
