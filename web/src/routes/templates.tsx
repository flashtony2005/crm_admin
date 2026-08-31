/**
 * 模板管理（/templates）
 *
 * 后端在 server/templates/<slug>/ 下以「目录」托管首页模板；注册表为
 * templates/manifest.json（slug / name / kind / port / note / active）。
 * - kind=app：外部已注册应用（coucouya:5199、fastshot:5197），不可删除；
 * - kind=upload：后台上传解包的 zip 包，存于 server/templates/<slug>/，可删。
 *
 * 本页负责：列出模板、上传 zip 新模板、激活/切换主模板、编辑元数据、删除上传类。
 * 所有写操作需 site.settings.update（Owner）。主端口的实际对外展示由部署侧
 * 按 site_settings.home_template 指向；本页「预览」链接可直接打开对应端口/路径。
 */
import { createFileRoute } from '@tanstack/react-router'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Card, Input, Label, toast } from '@heroui/react'

import { PageContainer } from '../components/layout/PageContainer'
import { api, getToken, request } from '../api/client'
import { usePermission } from '../hooks/usePermission'

interface Tpl {
  slug: string
  name: string
  kind: string
  port: string
  note: string
  createdAt: string
  active: boolean
  previewUrl: string
}

export const Route = createFileRoute('/templates')({
  component: TemplatesManager,
})

function TemplatesManager() {
  const { t } = useTranslation()
  const { has } = usePermission()
  const canEdit = has('site.settings.update')

  const [items, setItems] = useState<Tpl[]>([])
  const [active, setActive] = useState('')
  const [loaded, setLoaded] = useState(false)
  const [err, setErr] = useState('')
  const [busy, setBusy] = useState(false)

  // 上传表单
  const [file, setFile] = useState<File | null>(null)
  const [slug, setSlug] = useState('')
  const [name, setName] = useState('')
  const fileRef = useRef<HTMLInputElement>(null)

  const load = useCallback(async () => {
    try {
      const data = await api<{ items: Tpl[]; active: string }>('/api/public/templates')
      setItems(data.items || [])
      setActive(data.active || '')
      setErr('')
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setLoaded(true)
    }
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const activate = async (slug: string) => {
    if (!canEdit) return
    setBusy(true)
    try {
      await request(`/api/admin/templates/${slug}/activate`, { method: 'POST' })
      toast('已切换为主模板', { variant: 'success' })
      await load()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    } finally {
      setBusy(false)
    }
  }

  const remove = async (slug: string) => {
    if (!canEdit) return
    if (!window.confirm(`确认删除模板「${slug}」？该操作会移除其文件目录，且不可恢复。`)) return
    setBusy(true)
    try {
      await request(`/api/admin/templates/${slug}`, { method: 'DELETE' })
      toast('已删除', { variant: 'success' })
      await load()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    } finally {
      setBusy(false)
    }
  }

  const saveMeta = async (it: Tpl) => {
    if (!canEdit) return
    setBusy(true)
    try {
      await request(`/api/admin/templates/${it.slug}`, {
        method: 'PUT',
        body: JSON.stringify({ name: it.name, port: it.port, note: it.note }),
      })
      toast('已保存', { variant: 'success' })
      await load()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    } finally {
      setBusy(false)
    }
  }

  const upload = async () => {
    if (!canEdit) return
    if (!file) {
      toast('请先选择 zip 模板包', { variant: 'warning' })
      return
    }
    setBusy(true)
    try {
      const fd = new FormData()
      fd.append('file', file)
      if (slug.trim()) fd.append('slug', slug.trim())
      if (name.trim()) fd.append('name', name.trim())
      const token = getToken()
      const res = await fetch('/api/admin/templates', {
        method: 'POST',
        headers: token ? { Authorization: `Bearer ${token}` } : undefined,
        body: fd,
      })
      const b = await res.json().catch(() => ({ ok: false, error: `上传失败 (${res.status})` }))
      if (!res.ok || b.ok === false) throw new Error(b.error || '上传失败')
      toast(`已上传模板：${b.data?.slug || slug}`, { variant: 'success' })
      setFile(null)
      setSlug('')
      setName('')
      if (fileRef.current) fileRef.current.value = ''
      await load()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    } finally {
      setBusy(false)
    }
  }

  if (!loaded) {
    return <div className="py-6 text-sm text-default-400">{t('common.loading')}</div>
  }

  return (
    <PageContainer title="模板管理" subtitle="首页模板切换 / 上传(zip) / 主端口分配">
      <div className="max-w-3xl space-y-6 py-2">
        {err && (
          <div className="rounded-lg border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
            {err}
          </div>
        )}

        {!canEdit && (
          <div className="rounded-xl border border-default-200 bg-default-50 px-4 py-3 text-sm text-default-500">
            模板切换 / 上传为 Owner 权限；当前账号只能查看，无法修改。
          </div>
        )}

        {/* 上传新模板（zip 包，含 index.html + 资源） */}
        <Card className="border border-default-200 p-4 space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold">上传新模板</h3>
            <span className="text-[11px] text-default-400">zip 包 · 须含 index.html · ≤20MB</span>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>模板包（.zip）</Label>
              <input
                ref={fileRef}
                type="file"
                accept=".zip,application/zip"
                disabled={!canEdit || busy}
                onChange={(e: any) => setFile(e.target.files?.[0] ?? null)}
                className="block w-full text-sm text-default-600 file:mr-3 file:rounded-lg file:border-0 file:bg-primary/10 file:px-3 file:py-1.5 file:text-primary hover:file:bg-primary/20"
              />
              {file && <p className="text-[11px] text-default-500">已选：{file.name}</p>}
            </div>
            <div className="space-y-3">
              <div className="space-y-1.5">
                <Label>slug（可选，留空取文件名）</Label>
                <Input
                  value={slug}
                  onChange={(e: any) => setSlug(e.target.value)}
                  placeholder="my-template"
                  disabled={!canEdit || busy}
                />
              </div>
              <div className="space-y-1.5">
                <Label>显示名（可选）</Label>
                <Input
                  value={name}
                  onChange={(e: any) => setName(e.target.value)}
                  placeholder="我的模板"
                  disabled={!canEdit || busy}
                />
              </div>
            </div>
          </div>
          <div>
            <Button
              onPress={upload}
              isDisabled={!canEdit || busy || !file}
              size="sm"
            >
              上传并登记
            </Button>
          </div>
        </Card>

        {/* 模板列表 */}
        <div className="space-y-3">
          <h3 className="text-sm font-semibold">已注册模板（{items.length}）</h3>
          {items.map((it) => (
            <TemplateCard
              key={it.slug}
              tpl={it}
              isActive={it.slug === active}
              canEdit={canEdit}
              busy={busy}
              onActivate={() => activate(it.slug)}
              onDelete={() => remove(it.slug)}
              onSave={(next) => saveMeta(next)}
            />
          ))}
          {items.length === 0 && (
            <p className="py-4 text-sm text-default-400">暂无模板。</p>
          )}
        </div>
      </div>
    </PageContainer>
  )
}

function TemplateCard({
  tpl,
  isActive,
  canEdit,
  busy,
  onActivate,
  onDelete,
  onSave,
}: {
  tpl: Tpl
  isActive: boolean
  canEdit: boolean
  busy: boolean
  onActivate: () => void
  onDelete: () => void
  onSave: (next: Tpl) => void
}) {
  const [draft, setDraft] = useState<Tpl>(tpl)
  useEffect(() => setDraft(tpl), [tpl])

  const isUpload = tpl.kind === 'upload'

  return (
    <Card className={`border p-4 space-y-3 ${isActive ? 'border-primary ring-2 ring-primary/30' : 'border-default-200'}`}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <p className="truncate text-sm font-medium">{draft.name}</p>
            <span className="rounded-full bg-default-100 px-2 py-0.5 text-[11px] text-default-500">
              {isUpload ? '上传' : '外部应用'}
            </span>
            {isActive && (
              <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
                主端口 · 已激活
              </span>
            )}
          </div>
          <p className="mt-0.5 text-[11px] text-default-400">slug: {tpl.slug}</p>
        </div>
        <div className="flex shrink-0 gap-2">
          {!isActive && (
              <Button
                size="sm"
                variant="ghost"
                onPress={onActivate}
                isDisabled={!canEdit || busy}
              >
                设为主模板
              </Button>
          )}
          {isUpload && (
            <Button
              size="sm"
              variant="ghost"
              onPress={onDelete}
              isDisabled={!canEdit || busy}
              className="text-danger"
            >
              删除
            </Button>
          )}
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1.5">
          <Label>显示名</Label>
          <Input
            value={draft.name}
            onChange={(e: any) => setDraft({ ...draft, name: e.target.value })}
            disabled={!canEdit || busy}
          />
        </div>
        <div className="space-y-1.5">
          <Label>端口（外部应用）</Label>
          <Input
            value={draft.port}
            onChange={(e: any) => setDraft({ ...draft, port: e.target.value.replace(/\D/g, '') })}
            placeholder="如 5199"
            disabled={!canEdit || busy}
          />
        </div>
      </div>

      <div className="space-y-1.5">
        <Label>备注</Label>
        <Input
          value={draft.note}
          onChange={(e: any) => setDraft({ ...draft, note: e.target.value })}
          disabled={!canEdit || busy}
        />
      </div>

      <div className="flex flex-wrap items-center gap-3 pt-1">
        <a
          href={tpl.previewUrl}
          target="_blank"
          rel="noreferrer"
          className="text-[12px] text-primary underline hover:text-primary/80"
        >
          预览 →
        </a>
        {canEdit && (
          <Button
            size="sm"
            variant="ghost"
            onPress={() => onSave(draft)}
            isDisabled={busy || (draft.name === tpl.name && draft.port === tpl.port && draft.note === tpl.note)}
          >
            保存修改
          </Button>
        )}
      </div>
    </Card>
  )
}
