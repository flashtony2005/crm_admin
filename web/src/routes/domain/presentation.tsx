import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button } from '@heroui/react'

import {
  domainApi,
  type DvRender,
  type DvSite,
  type DvSiteTemplate,
  type DvSiteTheme,
} from '../../api/domain'
import type { FormFieldDef } from '../../api/cms'
import { CmsDataTable, type CmsColumn } from '../../components/cms/CmsDataTable'
import { CmsFormModal } from '../../components/cms/CmsFormModal'
import { CmsPageHeader } from '../../components/cms/CmsPageHeader'
import { Auth } from '../../components/cms/Auth'
import { P } from '../../config/permissions'

const SITE_FIELDS: FormFieldDef[] = [
  { key: 'name', label: '站点名称', type: 'text', required: true },
  { key: 'slug', label: 'Slug', type: 'text', required: true, placeholder: '如 coucouya' },
  { key: 'domain', label: '域名', type: 'text', placeholder: '如 coucouya.com' },
  { key: 'description', label: '描述', type: 'textarea' },
  { key: 'defaultLocale', label: '默认语言', type: 'text', defaultValue: 'zh-CN' },
  { key: 'timezone', label: '时区', type: 'text', defaultValue: 'Asia/Shanghai' },
  {
    key: 'status',
    label: '状态',
    type: 'select',
    options: ['draft', 'published', 'archived'].map((v) => ({ value: v, label: v })),
    defaultValue: 'published',
  },
]

function SitesPage() {
  const qc = useQueryClient()
  const [editing, setEditing] = useState<DvSite | null>(null)
  const [modalOpen, setModalOpen] = useState(false)
  const [selected, setSelected] = useState<DvSite | null>(null)
  const [render, setRender] = useState<DvRender | null>(null)
  const [rendering, setRendering] = useState(false)

  const sitesQ = useQuery({ queryKey: ['domain-sites'], queryFn: () => domainApi.sites.list() })
  const templatesQ = useQuery({ queryKey: ['domain-templates'], queryFn: () => domainApi.templates.list() })
  const themesQ = useQuery({ queryKey: ['domain-themes'], queryFn: () => domainApi.themes.list() })

  const siteId = selected?.id ?? ''
  const bindingsQ = useQuery({
    queryKey: ['domain-site-bindings', siteId],
    queryFn: async () => {
      if (!siteId) return { templates: [] as DvSiteTemplate[], themes: [] as DvSiteTheme[] }
      const [templates, themes] = await Promise.all([
        domainApi.sites.templates(siteId),
        domainApi.sites.themes(siteId),
      ])
      return { templates, themes }
    },
    enabled: !!siteId,
  })

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ['domain-sites'] })
    qc.invalidateQueries({ queryKey: ['domain-site-bindings'] })
  }

  const handleSubmit = async (values: Record<string, string | number>) => {
    const patch: Record<string, unknown> = {
      name: String(values.name),
      slug: String(values.slug),
      domain: String(values.domain ?? '').trim() || undefined,
      description: String(values.description ?? '').trim() || undefined,
      defaultLocale: String(values.defaultLocale ?? 'zh-CN').trim() || 'zh-CN',
      timezone: String(values.timezone ?? 'Asia/Shanghai').trim() || 'Asia/Shanghai',
      status: String(values.status ?? 'published'),
    }
    if (editing) await domainApi.sites.update(editing.id, patch)
    else await domainApi.sites.create(patch)
    refresh()
  }

  const handleDelete = async (row: DvSite) => {
    if (window.confirm(`确定删除站点「${row.name}」吗？`)) {
      await domainApi.sites.remove(row.id)
      if (selected?.id === row.id) setSelected(null)
      refresh()
    }
  }

  const handleBindTemplate = async (values: Record<string, string | number>) => {
    if (!siteId) return
    await domainApi.sites.bindTemplate(siteId, {
      templateId: String(values.templateId),
      route: String(values.route).trim() || '/',
      isDefault: values.isDefault === '1' || values.isDefault === 1,
    })
    refresh()
  }

  const handleBindTheme = async (values: Record<string, string | number>) => {
    if (!siteId) return
    await domainApi.sites.bindTheme(siteId, {
      themeId: String(values.themeId),
      isDefault: values.isDefault === '1' || values.isDefault === 1,
    })
    refresh()
  }

  const loadRender = async (route = '/') => {
    if (!siteId) return
    setRendering(true)
    try {
      setRender(await domainApi.sites.render(siteId, route))
    } finally {
      setRendering(false)
    }
  }

  const columns: CmsColumn<DvSite>[] = [
    {
      id: 'name',
      header: '站点',
      render: (r) => (
        <div className="min-w-0">
          <p className="font-medium text-os-text-primary">{r.name}</p>
          <p className="text-xs text-os-text-muted">{r.domain || r.slug}</p>
        </div>
      ),
    },
    {
      id: 'status',
      header: '状态',
      render: (r) => (
        <span
          className={`px-1.5 py-0.5 rounded-md text-xs font-medium ${
            r.status === 'published' ? 'bg-emerald-50 text-emerald-600' : 'bg-slate-100 text-slate-500'
          }`}
        >
          {r.status}
        </span>
      ),
    },
    {
      id: 'meta',
      header: '语言 / 时区',
      render: (r) => (
        <span className="text-xs text-os-text-muted">
          {r.defaultLocale || '—'} · {r.timezone || '—'}
        </span>
      ),
    },
  ]

  const bindTplFields: FormFieldDef[] = [
    {
      key: 'templateId',
      label: '模板',
      type: 'select',
      required: true,
      options: (templatesQ.data ?? []).map((t) => ({ value: t.id, label: `${t.name}（${t.type}）` })),
    },
    { key: 'route', label: '路由', type: 'text', defaultValue: '/', placeholder: '如 /article/:slug' },
    {
      key: 'isDefault',
      label: '设为默认',
      type: 'select',
      options: [
        { value: '1', label: '是' },
        { value: '0', label: '否' },
      ],
      defaultValue: '0',
    },
  ]

  const bindThemeFields: FormFieldDef[] = [
    {
      key: 'themeId',
      label: '主题',
      type: 'select',
      required: true,
      options: (themesQ.data ?? []).map((t) => ({ value: t.id, label: t.name })),
    },
    {
      key: 'isDefault',
      label: '设为默认',
      type: 'select',
      options: [
        { value: '1', label: '是' },
        { value: '0', label: '否' },
      ],
      defaultValue: '0',
    },
  ]

  return (
    <div className="p-1 md:p-2">
      <CmsPageHeader
        title="Domain · 站点与呈现"
        desc="Site 绑定 Template（怎么组合）与 Theme（怎么呈现），Renderer 按 Render Contract 输出。"
      />

      <CmsDataTable
        columns={columns}
        rows={sitesQ.data ?? []}
        rowKey={(r) => r.id}
        isLoading={sitesQ.isLoading}
        emptyIcon="🌐"
        emptyTitle="还没有站点"
        emptyHint="站点是呈现的载体：绑定模板路由与主题后即可渲染"
        actions={(row) => (
          <div className="flex gap-1.5">
            <Button variant="ghost" size="sm" onPress={() => setSelected(row)}>查看呈现</Button>
            <Auth perm={P.domainSiteUpdate} mode="disable">
              <Button variant="ghost" size="sm" onPress={() => { setEditing(row); setModalOpen(true) }}>编辑</Button>
            </Auth>
            <Auth perm={P.domainSiteDelete}>
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

      {selected ? (
        <div className="mt-4 grid gap-4 lg:grid-cols-2">
          {/* 左：绑定关系 */}
          <div className="rounded-xl border border-gray-200 bg-white p-4">
            <div className="flex items-center justify-between">
              <h3 className="font-semibold text-os-text-primary">「{selected.name}」绑定关系</h3>
              <span className="text-xs text-os-text-muted">{selected.slug}</span>
            </div>

            <p className="mt-3 text-xs font-medium text-os-text-secondary">模板路由</p>
            <ul className="mt-1 space-y-1">
              {(bindingsQ.data?.templates ?? []).map((b) => (
                <li key={b.templateId + b.route} className="flex items-center gap-2 text-sm">
                  <code className="px-1.5 py-0.5 rounded bg-slate-100 text-xs">{b.route}</code>
                  <span className="text-os-text-primary">{b.template?.name ?? '…'}</span>
                  {b.isDefault ? (
                    <span className="px-1 py-0.5 rounded bg-indigo-50 text-indigo-600 text-[10px]">默认</span>
                  ) : null}
                  <Auth perm={P.domainSiteUpdate}>
                    <button
                      className="ml-auto text-xs text-os-danger-text hover:underline"
                      onClick={() => domainApi.sites.unbindTemplate(selected.id, b.templateId).then(refresh)}
                    >
                      解绑
                    </button>
                  </Auth>
                </li>
              ))}
              {!bindingsQ.data?.templates?.length ? <li className="text-xs text-os-text-muted">未绑定模板</li> : null}
            </ul>
            <Auth perm={P.domainSiteUpdate} mode="disable">
              <div className="mt-3 flex gap-2">
                <TemplateBindInline fields={bindTplFields} onSubmit={handleBindTemplate} />
              </div>
            </Auth>

            <p className="mt-4 text-xs font-medium text-os-text-secondary">主题</p>
            <ul className="mt-1 space-y-1">
              {(bindingsQ.data?.themes ?? []).map((b) => (
                <li key={b.themeId} className="flex items-center gap-2 text-sm">
                  <span className="text-os-text-primary">{b.theme?.name ?? '…'}</span>
                  {b.isDefault ? (
                    <span className="px-1 py-0.5 rounded bg-indigo-50 text-indigo-600 text-[10px]">默认</span>
                  ) : null}
                  <Auth perm={P.domainSiteUpdate}>
                    <button
                      className="ml-auto text-xs text-os-danger-text hover:underline"
                      onClick={() => domainApi.sites.unbindTheme(selected.id, b.themeId).then(refresh)}
                    >
                      解绑
                    </button>
                  </Auth>
                </li>
              ))}
              {!bindingsQ.data?.themes?.length ? <li className="text-xs text-os-text-muted">未绑定主题</li> : null}
            </ul>
            <Auth perm={P.domainSiteUpdate} mode="disable">
              <div className="mt-3">
                <ThemeBindInline fields={bindThemeFields} onSubmit={handleBindTheme} />
              </div>
            </Auth>

            <p className="mt-4 text-xs font-medium text-os-text-secondary">模板库（只读）</p>
            <ul className="mt-1 space-y-0.5">
              {(templatesQ.data ?? []).map((t) => (
                <li key={t.id} className="flex items-center gap-2 text-xs">
                  <span className="text-os-text-primary">{t.name}</span>
                  <span className="text-os-text-muted">{t.type} · v{t.latestVersion}</span>
                </li>
              ))}
            </ul>
            <p className="mt-2 text-xs font-medium text-os-text-secondary">主题库（只读）</p>
            <ul className="mt-1 space-y-0.5">
              {(themesQ.data ?? []).map((t) => (
                <li key={t.id} className="flex items-center gap-2 text-xs">
                  <span className="text-os-text-primary">{t.name}</span>
                  <span className="text-os-text-muted">v{t.latestVersion}</span>
                </li>
              ))}
            </ul>
          </div>

          {/* 右：Render Contract 预览 */}
          <div className="rounded-xl border border-gray-200 bg-white p-4">
            <div className="flex items-center justify-between">
              <h3 className="font-semibold text-os-text-primary">Render Contract</h3>
              <div className="flex gap-2">
                <Button variant="ghost" size="sm" isDisabled={rendering} onPress={() => loadRender('/')}>
                  {rendering ? '渲染中…' : '渲染首页'}
                </Button>
                <Button variant="ghost" size="sm" isDisabled={rendering} onPress={() => loadRender('/article/:slug')}>
                  {rendering ? '渲染中…' : '渲染文章'}
                </Button>
              </div>
            </div>

            {render ? (
              <div className="mt-3 space-y-3 text-sm">
                <div className="flex flex-wrap gap-1.5">
                  <span className="px-1.5 py-0.5 rounded bg-slate-100 font-mono text-xs">{render.schema}</span>
                  <span className="px-1.5 py-0.5 rounded bg-slate-100 text-xs">sections: {render.sections?.length ?? 0}</span>
                  {render.theme ? (
                    <span className="px-1.5 py-0.5 rounded bg-slate-100 text-xs">
                      theme tokens: {Object.keys(render.theme.tokens ?? {}).length}
                    </span>
                  ) : null}
                  {render.template ? (
                    <span className="px-1.5 py-0.5 rounded bg-slate-100 text-xs">template v{render.template.version}</span>
                  ) : null}
                </div>
                <ul className="space-y-1">
                  {render.sections?.map((s) => (
                    <li key={s.id} className="flex items-center gap-2 text-xs">
                      <code className="px-1.5 py-0.5 rounded bg-violet-50 text-violet-600">{s.component}</code>
                      <span className="text-os-text-muted truncate">{JSON.stringify(s.binding)}</span>
                    </li>
                  ))}
                </ul>
                {render.navigation?.nav?.length ? (
                  <p className="text-xs text-os-text-muted">navigation: {render.navigation.nav.length} 项</p>
                ) : null}
              </div>
            ) : (
              <p className="mt-3 text-sm text-os-text-muted">
                选择路由点击渲染，查看输出给外部 Renderer 的呈现契约（与前端无关）。
              </p>
            )}
          </div>
        </div>
      ) : null}

      <CmsFormModal
        title={editing ? `编辑站点：${editing.name}` : '新建站点'}
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        onSubmit={handleSubmit}
        fields={SITE_FIELDS}
        initial={editing ?? undefined}
      />
    </div>
  )
}

/** 绑定模板的轻量内联表单（免弹窗，保持 master-detail 清爽） */
function TemplateBindInline({
  fields,
  onSubmit,
}: {
  fields: FormFieldDef[]
  onSubmit: (v: Record<string, string | number>) => Promise<void>
}) {
  const [templateId, setTemplateId] = useState('')
  const [route, setRoute] = useState('/')
  const [saving, setSaving] = useState(false)
  const tplField = fields[0]
  return (
    <div className="flex gap-2">
      <select
        className="w-36 border border-gray-200 rounded-lg px-2 py-1.5 text-xs outline-none focus:border-indigo-400 bg-white"
        value={templateId}
        onChange={(e) => setTemplateId(e.target.value)}
      >
        <option value="">选择模板…</option>
        {(tplField.options ?? []).map((o) => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
      </select>
      <input
        className="w-32 border border-gray-200 rounded-lg px-2 py-1.5 text-xs outline-none focus:border-indigo-400"
        value={route}
        onChange={(e) => setRoute(e.target.value)}
        placeholder="路由"
      />
      <Button
        variant="primary" size="sm"
        isDisabled={saving || !templateId}
        onPress={async () => {
          if (!templateId) return
          setSaving(true)
          try {
            await onSubmit({ templateId, route, isDefault: '0' })
          } finally {
            setSaving(false)
          }
        }}
      >
        {saving ? '绑定中…' : '绑定'}
      </Button>
    </div>
  )
}

/** 绑定主题的轻量内联表单 */
function ThemeBindInline({
  fields,
  onSubmit,
}: {
  fields: FormFieldDef[]
  onSubmit: (v: Record<string, string | number>) => Promise<void>
}) {
  const [themeId, setThemeId] = useState('')
  const [saving, setSaving] = useState(false)
  const themeField = fields[0]
  return (
    <div className="flex gap-2">
      <select
        className="w-36 border border-gray-200 rounded-lg px-2 py-1.5 text-xs outline-none focus:border-indigo-400 bg-white"
        value={themeId}
        onChange={(e) => setThemeId(e.target.value)}
      >
        <option value="">选择主题…</option>
        {(themeField.options ?? []).map((o) => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
      </select>
      <Button
        variant="primary" size="sm"
        isDisabled={saving || !themeId}
        onPress={async () => {
          if (!themeId) return
          setSaving(true)
          try {
            await onSubmit({ themeId, isDefault: '1' })
          } finally {
            setSaving(false)
          }
        }}
      >
        {saving ? '绑定中…' : '绑定默认主题'}
      </Button>
    </div>
  )
}

export const Route = createFileRoute('/domain/presentation')({
  component: SitesPage,
})
