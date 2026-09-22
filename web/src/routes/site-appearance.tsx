/**
 * 站点外观（独立菜单页 /site-appearance）
 *
 * 从「设置 → 站点外观」tab 提升为一级菜单项，便于高频维护：
 * - 主题 / 模板 / 品牌 / 主页风格：需 `site.settings.update`（Owner）
 * - 首页区块：需 `content.sections.*`
 * - 导航与页脚链接：需 `site.nav.*`（nav_links 表）
 * - 主页置顶文章：需 `site.homePins.*`（home_pins 表）
 * 各区块按权限独立显隐，Editor 虽不能改主题，仍可维护导航与置顶。
 *
 * 顶部「主页可视化编辑」嵌的是真实公开主页（iframe + postMessage 桥）：
 * 在预览里点元素即弹出对应内容的编辑框，改完预览即时刷新，无需猜字段在哪。
 */
import { useState, useEffect, useCallback } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { Button, Switch, Input, Card, Label, toast } from '@heroui/react'
import { PageContainer } from '../components/layout/PageContainer'
import { request, api } from '../api/client'
import { usePermission } from '../hooks/usePermission'
import { JsonForm } from '../components/cms/JsonForm'
import { SiteLivePreview } from '../components/cms/SiteLivePreview'
import { SITE_THEMES } from '../themes/siteThemes'
import { SITE_TEMPLATES } from '../themes/siteTemplates'

// ── Inline icons ──────────────────────────────────────────────
const Icon = ({ children, size = 16 }: { children: React.ReactNode; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
    strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="shrink-0">
    {children}
  </svg>
)

const PaletteIcon = () => <Icon><path d="M12 3a9 9 0 1 0 0 18 2 2 0 0 0 2-2 2 2 0 0 1 2-2h1a4 4 0 0 0 4-4 9 9 0 0 0-9-8z"/><circle cx="7.5" cy="10.5" r="1"/><circle cx="12" cy="7.5" r="1"/><circle cx="16.5" cy="10.5" r="1"/></Icon>

// 公开主页的风格变体。id / swatch 必须与 coucouya/src/themes.ts 的 THEMES 保持一致
// （那是权威定义）；新增一档需要两边同时加，否则后台选了前端不认。
const HOME_THEMES: { id: string; name: string; swatch: [string, string, string]; group: string }[] = [
  { id: 'duck', name: '暖奶白可可鸭', swatch: ['#FBF8F1', '#FFFFFF', '#D99B1F'], group: '品牌' },
  { id: 'mono', name: '极简黑白', swatch: ['#FFFFFF', '#F0F0F0', '#111111'], group: '品牌' },
  { id: 'emerald', name: '翡翠绿', swatch: ['#F4FBF7', '#E6F6EE', '#0F9D63'], group: '品牌' },
  { id: 'ocean', name: '深空蓝', swatch: ['#0A1020', '#16223B', '#3B82F6'], group: '品牌' },
  { id: 'poema', name: '纸感衬线', swatch: ['#FAF8F4', '#FFFFFF', '#1A1A1A'], group: '内容' },
  { id: 'vivre', name: '杂志高对比', swatch: ['#FFFFFF', '#F5F3F0', '#C8102E'], group: '内容' },
  { id: 'retrospect', name: '深底影像', swatch: ['#101012', '#1A1A1E', '#FAFAFA'], group: '内容' },
  { id: 'nook', name: '暖木双栏', swatch: ['#F7F3EA', '#FFFFFF', '#8B5E3C'], group: '生活' },
  { id: 'tsubaki', name: '椿日式', swatch: ['#FDF9F7', '#FFFFFF', '#B23A48'], group: '生活' },
  { id: 'aether', name: '暖棕叙事', swatch: ['#FBF6EF', '#FFFFFF', '#A0522D'], group: '生活' },
]

// 公开主页的版式预设（决定区块顺序与骨架，与上面的风格正交）。
// **这只是离线兜底**：权威目录在服务端 `server/src/presets.rs`，运行时由
// `/api/public/site` 的 `homePresets` 下发（见下方 presetCatalog）。后端不可用、
// 或旧版后端不带该字段时，才回落到这里，避免页面开天窗。
// `bars` 只是后台里的骨架缩略示意，`sidebar` 为真时缩略图右侧多画一块侧栏。
interface PresetMeta {
  id: string
  name: string
  desc: string
  bars: number[]
  sidebar?: boolean
}
const FALLBACK_HOME_PRESETS: PresetMeta[] = [
  {
    id: 'classic',
    name: '经典版式',
    desc: '主视觉 → 组织 → 代表文章 → 系列 → Web3，信息最全',
    bars: [100, 58, 84, 50, 70],
  },
  {
    id: 'editorial',
    name: '杂志编辑',
    desc: '内容前置：大标题主视觉 + 文章栅格 + 系列分区',
    bars: [100, 88, 72, 54, 62],
  },
  {
    id: 'gallery',
    name: '影像优先',
    desc: '深底 + 两列大图，封面图主导，去掉旁枝',
    bars: [58, 96, 74, 52],
  },
  {
    id: 'sidebar',
    name: '侧栏双栏',
    desc: '主内容 + 常驻侧栏（订阅位 / 最近更新 / 目录）',
    bars: [72, 88, 76, 64],
    sidebar: true,
  },
  {
    id: 'minimal',
    name: '极简单栏',
    desc: '只留主视觉与单列文章流，阅读动线最短',
    bars: [100, 76],
  },
]

interface SectionRow {
  id: string
  slug: string
  title: string
  subtitle: string | null
  icon: string | null
  body: unknown
  sort: number
  updatedAt: string
}

/** 深拷贝（表单编辑与「重置」都基于副本，避免直接改到列表里的对象） */
function cloneJson<T>(v: T): T {
  return JSON.parse(JSON.stringify(v ?? null)) as T
}

// 单个首页区块的可编辑卡片（标题 / 副标题 / 结构化正文表单）
function BlockCard({ row, onChanged }: { row: SectionRow; onChanged: () => void }) {
  const { t } = useTranslation()
  const [title, setTitle] = useState(row.title)
  const [subtitle, setSubtitle] = useState(row.subtitle ?? '')
  // body 以对象形式受控编辑（不再是 JSON 文本），保存时直接PUT，无需解析
  const [body, setBody] = useState<unknown>(() => cloneJson(row.body ?? {}))
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [showRaw, setShowRaw] = useState(false)

  useEffect(() => {
    setTitle(row.title)
    setSubtitle(row.subtitle ?? '')
    setBody(cloneJson(row.body ?? {}))
    setDirty(false)
  }, [row])

  const onField = (fn: () => void) => {
    fn()
    setDirty(true)
  }

  const save = async () => {
    setSaving(true)
    try {
      await request(`/api/sections/${row.id}`, {
        method: 'PUT',
        body: JSON.stringify({ title, subtitle, body }),
      })
      toast('首页区块已保存', { variant: 'success' })
      setDirty(false)
      onChanged()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    } finally {
      setSaving(false)
    }
  }

  const reset = () => {
    setTitle(row.title)
    setSubtitle(row.subtitle ?? '')
    setBody(cloneJson(row.body ?? {}))
    setDirty(false)
  }

  return (
    <Card className="border border-default-200 p-4 space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-xs font-bold uppercase tracking-wide text-default-400">{row.slug}</span>
        {dirty && <span className="text-[11px] text-warning">● 未保存</span>}
      </div>
      <div className="space-y-1.5">
        <Label>区块标题</Label>
        <Input
          value={title}
          onChange={(e: any) => onField(() => setTitle(e.target.value))}
          placeholder="如：关于可可鸭"
        />
      </div>
      <div className="space-y-1.5">
        <Label>区块副标题</Label>
        <Input
          value={subtitle}
          onChange={(e: any) => onField(() => setSubtitle(e.target.value))}
          placeholder="可选"
        />
      </div>

      {/* 结构化正文：按字段填写，数组可增删排序；无需接触 JSON 语法 */}
      <div className="space-y-2 pt-1">
        <div className="flex items-center justify-between">
          <Label>区块内容</Label>
          <button
            type="button"
            className="text-[11px] text-default-400 hover:text-default-700"
            onClick={() => setShowRaw((v) => !v)}
          >
            {showRaw ? '收起原始 JSON' : '查看原始 JSON'}
          </button>
        </div>
        <div className="rounded-lg border border-default-200 bg-default-50/60 p-3">
          <JsonForm value={body} onChange={(v) => onField(() => setBody(v))} />
        </div>
        {showRaw && (
          <pre className="max-h-64 overflow-auto rounded-lg border border-default-200 bg-default-100 p-3 font-mono text-[11px] leading-relaxed">
            {JSON.stringify(body, null, 2)}
          </pre>
        )}
        <p className="text-[11px] text-default-400">
          按 slug 合并进首页：about→hero/kicker/tagline，org→affiliation/pillars，lab→writing/series，web3→web3。
        </p>
      </div>

      {dirty && (
        <div className="flex gap-3 pt-1">
          <Button onPress={save} isDisabled={saving} size="sm">
            {t('common.save')}
          </Button>
          <Button variant="outline" size="sm" onPress={reset}>
            {t('common.cancel')}
          </Button>
        </div>
      )}
    </Card>
  )
}

// 首页区块管理：拉取 /api/sections（content.sections.view），按 slug 列出 4 个区块并就地编辑
function HomeBlocksEditor() {
  const { t } = useTranslation()
  const [rows, setRows] = useState<SectionRow[]>([])
  const [loaded, setLoaded] = useState(false)
  const [err, setErr] = useState('')

  const load = useCallback(() => {
    setLoaded(false)
    api<SectionRow[]>('/api/sections')
      .then((data) => {
        setRows(data || [])
        setLoaded(true)
        setErr('')
      })
      .catch((e: Error) => {
        setErr(e.message)
        setLoaded(true)
      })
  }, [])

  useEffect(() => load(), [load])

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-base font-semibold flex items-center gap-2">
            <PaletteIcon /> 首页区块
          </h3>
          <p className="text-xs text-default-400 mt-0.5">
            关于 / 组织 / 实验 / Web3 四块内容，存为独立 CMS 资源表，公开主页实时读取。
          </p>
        </div>
        <Button variant="ghost" size="sm" onPress={load} isDisabled={!loaded}>
          {t('common.refresh')}
        </Button>
      </div>

      {!loaded && <div className="py-6 text-default-400 text-sm">{t('common.loading')}</div>}
      {loaded && err && (
        <div className="py-4 rounded-medium bg-warning-50 dark:bg-warning-900/20 px-4 text-warning-600 dark:text-warning-400 text-sm space-y-1">
          {/未知资源|401|Unauthorized/i.test(err) ? (
            <>
              <div className="font-medium">后台服务尚未包含「首页区块」模块</div>
              <div className="text-default-500">
                请运行 <code className="text-xs">server\build_sections.cmd</code>{' '}
                重新编译并自动重启后台（约 3-5 分钟），完成后点右上角「刷新」。
              </div>
            </>
          ) : (
            err
          )}
        </div>
      )}
      {loaded && !err && rows.length === 0 && (
        <div className="py-4 text-default-400 text-sm">暂无区块，请先启动后台以生成默认区块。</div>
      )}

      <div className="space-y-4">
        {rows.map((row) => (
          <BlockCard key={row.id} row={row} onChanged={load} />
        ))}
      </div>
    </div>
  )
}

// ── 导航与页脚链接（nav_links 表，site.nav.*）──────────────
// 通用资源网关 /api/navlinks 提供 CRUD；列表按 updated_at DESC 返回，
// 此处按 sort 重排，保证显示顺序即导航顺序。
interface NavLinkRow {
  id: string
  grp: string
  label: string
  href: string
  target: string
  sort: number
  enabled: boolean
  updatedAt?: string
}

const LinkIcon2 = () => (
  <Icon><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></Icon>
)

function NavLinksEditor() {
  const { t } = useTranslation()
  const [rows, setRows] = useState<NavLinkRow[]>([])
  const [loaded, setLoaded] = useState(false)
  const [err, setErr] = useState('')
  const [draft, setDraft] = useState<{ grp: string; label: string; href: string; target: string }>({
    grp: 'nav',
    label: '',
    href: '',
    target: '',
  })

  const load = useCallback(() => {
    setLoaded(false)
    api<NavLinkRow[]>('/api/navlinks')
      .then((data) => {
        // 通用网关按 updated_at DESC 返回 → 按 (grp, sort) 重排
        const sorted = [...(data || [])].sort((a, b) =>
          a.grp === b.grp ? a.sort - b.sort : (a.grp < b.grp ? -1 : 1),
        )
        setRows(sorted)
        setLoaded(true)
        setErr('')
      })
      .catch((e: Error) => {
        setErr(e.message)
        setLoaded(true)
      })
  }, [])

  useEffect(() => load(), [load])

  const saveRow = async (row: NavLinkRow, patchBody: Partial<NavLinkRow>) => {
    await request(`/api/navlinks/${row.id}`, {
      method: 'PUT',
      body: JSON.stringify(patchBody),
    })
    load()
  }

  const toggleEnabled = (row: NavLinkRow) =>
    saveRow(row, { enabled: !row.enabled }).catch((e: Error) => toast(e.message, { variant: 'danger' }))

  const move = (row: NavLinkRow, dir: -1 | 1) => {
    const group = rows.filter((r) => r.grp === row.grp).sort((a, b) => a.sort - b.sort)
    const idx = group.findIndex((r) => r.id === row.id)
    const swap = group[idx + dir]
    if (!swap) return
    const a = row.sort
    const b = swap.sort
    // 两项互换 sort（临时用负值避免唯一冲突）
    saveRow(row, { sort: -1 }).then(async () => {
      try {
        await saveRow(swap, { sort: a })
        await saveRow(row, { sort: b })
      } catch (e) {
        toast((e as Error).message, { variant: 'danger' })
      }
      load()
    })
  }

  const remove = async (row: NavLinkRow) => {
    await request(`/api/navlinks/${row.id}`, { method: 'DELETE' })
    load()
  }

  const add = async () => {
    if (!draft.label.trim() || !draft.href.trim()) {
      toast('请填写链接名称和地址', { variant: 'warning' })
      return
    }
    const maxSort = Math.max(0, ...rows.filter((r) => r.grp === draft.grp).map((r) => r.sort))
    try {
      await request('/api/navlinks', {
        method: 'POST',
        body: JSON.stringify({
          grp: draft.grp,
          label: draft.label.trim(),
          href: draft.href.trim(),
          target: draft.target,
          sort: maxSort + 1,
          enabled: true,
        }),
      })
      setDraft({ grp: draft.grp, label: '', href: '', target: '' })
      load()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    }
  }

  const renderGroup = (grp: 'nav' | 'footer', title: string) => {
    const group = rows.filter((r) => r.grp === grp)
    return (
      <div className="space-y-2">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <LinkIcon2 /> {title}
          <span className="text-xs font-normal text-default-400">（{group.length} 条）</span>
        </div>
        {group.length === 0 && <div className="text-xs text-default-400 py-1">暂无链接</div>}
        {group.map((row) => (
          <div key={row.id} className="flex items-center gap-2 rounded-lg border border-default-200 bg-default-50/60 px-2.5 py-2">
            <div className="flex flex-col gap-0.5">
              <button type="button" className="text-default-400 hover:text-default-700 leading-none" onClick={() => move(row, -1)} aria-label="上移">▲</button>
              <button type="button" className="text-default-400 hover:text-default-700 leading-none" onClick={() => move(row, 1)} aria-label="下移">▼</button>
            </div>
            <div className="flex-1 min-w-0 space-y-1.5">
              <div className="flex items-center gap-2">
                <Input
                  aria-label="名称"
                  value={row.label}
                  onChange={(e: any) => {
                    row.label = e.target.value
                    setRows([...rows])
                  }}
                  onBlur={() => row.label.trim() && saveRow(row, { label: row.label.trim() }).catch((e) => toast(e.message, { variant: 'danger' }))}
                  placeholder="名称"
                />
                <Input
                  aria-label="地址"
                  value={row.href}
                  onChange={(e: any) => {
                    row.href = e.target.value
                    setRows([...rows])
                  }}
                  onBlur={() => row.href.trim() && saveRow(row, { href: row.href.trim() }).catch((e) => toast(e.message, { variant: 'danger' }))}
                  placeholder="#about / /tag/xxx / https://…"
                />
              </div>
              <div className="flex items-center gap-2">
                <label className="flex items-center gap-1.5 text-xs text-default-500">
                  <input
                    type="checkbox"
                    checked={row.target === '_blank'}
                    onChange={(e) => saveRow(row, { target: e.target.checked ? '_blank' : '' }).catch((err) => toast(err.message, { variant: 'danger' }))}
                  />
                  新窗口打开
                </label>
                <Switch
                  size="sm"
                  isSelected={row.enabled}
                  onChange={() => toggleEnabled(row)}
                  aria-label="启用"
                />
                <Button size="sm" variant="ghost" onPress={() => remove(row).catch((e) => toast(e.message, { variant: 'danger' }))}>
                  删除
                </Button>
              </div>
            </div>
          </div>
        ))}
      </div>
    )
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-base font-semibold flex items-center gap-2">
            <LinkIcon2 /> 导航与页脚链接
          </h3>
          <p className="text-xs text-default-400 mt-0.5">
            存为 nav_links 表，公开站点经 /api/public/nav 读取；改动即时生效。
          </p>
        </div>
        <Button variant="ghost" size="sm" onPress={load} isDisabled={!loaded}>
          {t('common.refresh')}
        </Button>
      </div>

      {!loaded && <div className="py-4 text-default-400 text-sm">{t('common.loading')}</div>}
      {loaded && err && (
        <div className="py-3 rounded-medium bg-warning-50 dark:bg-warning-900/20 px-4 text-warning-600 dark:text-warning-400 text-sm">
          {/未知资源|401|Unauthorized/i.test(err) ? '后台服务尚未包含「导航与页脚链接」模块，请重新编译 cms-server 后刷新。' : err}
        </div>
      )}
      {loaded && !err && (
        <>
          <div className="grid sm:grid-cols-2 gap-5">
            {renderGroup('nav', '顶部导航')}
            {renderGroup('footer', '页脚链接')}
          </div>

          {/* 新增 */}
          <div className="rounded-lg border border-dashed border-default-300 p-3 space-y-2">
            <div className="flex items-center gap-2">
              <select
                value={draft.grp}
                onChange={(e) => setDraft({ ...draft, grp: e.target.value })}
                className="rounded-lg border border-default-200 bg-default-50 px-2 py-1.5 text-sm outline-none focus:border-primary"
              >
                <option value="nav">顶部导航</option>
                <option value="footer">页脚链接</option>
              </select>
              <Input aria-label="新增名称" value={draft.label} onChange={(e: any) => setDraft({ ...draft, label: e.target.value })} placeholder="名称" />
              <Input aria-label="新增地址" value={draft.href} onChange={(e: any) => setDraft({ ...draft, href: e.target.value })} placeholder="地址 #about / /tag/xxx / https://…" />
              <Button size="sm" onPress={add}>新增</Button>
            </div>
          </div>
        </>
      )}
    </div>
  )
}

// ── 主页置顶文章（home_pins 表，site.homePins.*）──────────────
// 通用资源网关 /api/home_pins 提供 CRUD（不联表），文章标题由本组件
// 自行用 /api/articles 映射；公开站点经 /api/public/home-pins 消费。
interface HomePinRow {
  id: string
  slot: string
  articleId: string
  sort: number
  enabled: boolean
}

interface ArticleBrief {
  id: string
  title: string
  status: string
}

const PinIcon = () => (
  <Icon><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></Icon>
)

function HomePinsEditor() {
  const { t } = useTranslation()
  const [rows, setRows] = useState<HomePinRow[]>([])
  const [articles, setArticles] = useState<ArticleBrief[]>([])
  const [loaded, setLoaded] = useState(false)
  const [err, setErr] = useState('')
  const [draftSlot, setDraftSlot] = useState('writing')
  const [draftArticle, setDraftArticle] = useState('')

  const load = useCallback(() => {
    setLoaded(false)
    Promise.all([
      api<HomePinRow[]>('/api/home_pins'),
      api<ArticleBrief[]>('/api/articles'),
    ])
      .then(([pins, arts]) => {
        const sorted = [...(pins || [])].sort((a, b) =>
          a.slot === b.slot ? a.sort - b.sort : a.slot < b.slot ? -1 : 1,
        )
        setRows(sorted)
        setArticles((arts || []).filter((a) => a.status === 'published'))
        setLoaded(true)
        setErr('')
      })
      .catch((e: Error) => {
        setErr(e.message)
        setLoaded(true)
      })
  }, [])

  useEffect(() => load(), [load])

  const titleOf = (articleId: string) =>
    articles.find((a) => a.id === articleId)?.title || `（未知文章 ${articleId.slice(0, 8)}）`

  const saveRow = async (row: HomePinRow, patchBody: Partial<HomePinRow>) => {
    await request(`/api/home_pins/${row.id}`, {
      method: 'PUT',
      body: JSON.stringify(patchBody),
    })
    load()
  }

  const move = (row: HomePinRow, dir: -1 | 1) => {
    const group = rows.filter((r) => r.slot === row.slot).sort((a, b) => a.sort - b.sort)
    const idx = group.findIndex((r) => r.id === row.id)
    const swap = group[idx + dir]
    if (!swap) return
    const a = row.sort
    const b = swap.sort
    saveRow(row, { sort: -1 }).then(async () => {
      try {
        await saveRow(swap, { sort: a })
        await saveRow(row, { sort: b })
      } catch (e) {
        toast((e as Error).message, { variant: 'danger' })
      }
      load()
    })
  }

  const remove = async (row: HomePinRow) => {
    await request(`/api/home_pins/${row.id}`, { method: 'DELETE' })
    load()
  }

  const add = async () => {
    if (!draftArticle) {
      toast('请选择要置顶的文章', { variant: 'warning' })
      return
    }
    // UNIQUE(tenant_id, slot, article_id)：同展示位不可重复固定同一篇
    if (rows.some((r) => r.slot === draftSlot && r.articleId === draftArticle)) {
      toast('该文章已在当前展示位中', { variant: 'warning' })
      return
    }
    const maxSort = Math.max(0, ...rows.filter((r) => r.slot === draftSlot).map((r) => r.sort))
    try {
      await request('/api/home_pins', {
        method: 'POST',
        body: JSON.stringify({
          slot: draftSlot,
          articleId: draftArticle,
          sort: maxSort + 1,
          enabled: true,
        }),
      })
      setDraftArticle('')
      load()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    }
  }

  const renderSlot = (slot: string, title: string) => {
    const group = rows.filter((r) => r.slot === slot)
    return (
      <div className="space-y-2">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <PinIcon /> {title}
          <span className="text-xs font-normal text-default-400">（{group.length} 篇）</span>
        </div>
        {group.length === 0 && (
          <div className="text-xs text-default-400 py-1">
            未配置，公开站点将回落为 featured 文章
          </div>
        )}
        {group.map((row) => (
          <div key={row.id} className="flex items-center gap-2 rounded-lg border border-default-200 bg-default-50/60 px-2.5 py-2">
            <div className="flex flex-col gap-0.5">
              <button type="button" className="text-default-400 hover:text-default-700 leading-none" onClick={() => move(row, -1)} aria-label="上移">▲</button>
              <button type="button" className="text-default-400 hover:text-default-700 leading-none" onClick={() => move(row, 1)} aria-label="下移">▼</button>
            </div>
            <div className="flex-1 min-w-0 truncate text-sm" title={row.articleId}>
              {titleOf(row.articleId)}
            </div>
            <Switch
              size="sm"
              isSelected={row.enabled}
              onChange={() =>
                saveRow(row, { enabled: !row.enabled }).catch((e) =>
                  toast(e.message, { variant: 'danger' }),
                )
              }
              aria-label="启用"
            />
            <Button size="sm" variant="ghost" onPress={() => remove(row).catch((e) => toast(e.message, { variant: 'danger' }))}>
              移除
            </Button>
          </div>
        ))}
      </div>
    )
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-base font-semibold flex items-center gap-2">
            <PinIcon /> 主页置顶文章
          </h3>
          <p className="text-xs text-default-400 mt-0.5">
            精确编排「哪几篇上主页、什么顺序」；未配置时公开站点回落为 featured 文章。
          </p>
        </div>
        <Button variant="ghost" size="sm" onPress={load} isDisabled={!loaded}>
          {t('common.refresh')}
        </Button>
      </div>

      {!loaded && <div className="py-4 text-default-400 text-sm">{t('common.loading')}</div>}
      {loaded && err && (
        <div className="py-3 rounded-medium bg-warning-50 dark:bg-warning-900/20 px-4 text-warning-600 dark:text-warning-400 text-sm">
          {/未知资源|401|Unauthorized/i.test(err) ? '后台服务尚未包含「主页置顶文章」模块，请重新编译 cms-server 后刷新。' : err}
        </div>
      )}
      {loaded && !err && (
        <>
          <div className="grid sm:grid-cols-2 gap-5">
            {renderSlot('writing', '代表文章')}
            {renderSlot('series', '系列')}
          </div>

          {/* 新增置顶 */}
          <div className="rounded-lg border border-dashed border-default-300 p-3">
            <div className="flex items-center gap-2">
              <select
                value={draftSlot}
                onChange={(e) => setDraftSlot(e.target.value)}
                className="rounded-lg border border-default-200 bg-default-50 px-2 py-1.5 text-sm outline-none focus:border-primary"
              >
                <option value="writing">代表文章</option>
                <option value="series">系列</option>
              </select>
              <select
                value={draftArticle}
                onChange={(e) => setDraftArticle(e.target.value)}
                className="flex-1 min-w-0 rounded-lg border border-default-200 bg-default-50 px-2 py-1.5 text-sm outline-none focus:border-primary"
              >
                <option value="">选择已发布文章…</option>
                {articles.map((a) => (
                  <option key={a.id} value={a.id}>{a.title || a.id}</option>
                ))}
              </select>
              <Button size="sm" onPress={add}>置顶</Button>
            </div>
            {articles.length === 0 && (
              <p className="text-[11px] text-default-400 mt-2">暂无已发布文章，请先发布文章后再来置顶。</p>
            )}
          </div>
        </>
      )}
    </div>
  )
}

// ── Site appearance (Owner only: site.settings.update) ─────────
function AppearanceBranding() {
  const { t } = useTranslation()
  const [theme, setTheme] = useState<string>('paper')
  const [template, setTemplate] = useState<string>('default')
  const [title, setTitle] = useState<string>('LightPress')
  const [tagline, setTagline] = useState<string>('')
  // 公开主页（coucouya）风格键：与本后台 theme 解耦，独立于 sepia/paper 等
  // 空串 = 不指定，跟随版式预设自带的建议风格（见 coucouya/src/App.tsx 的优先级链）
  const [homeTheme, setHomeTheme] = useState<string>('')
  // 公开主页的版式预设（与 homeTheme 正交：预设管排布，风格管配色）
  const [homePreset, setHomePreset] = useState<string>('classic')
  // 版式目录：优先用后端下发的（单一权威 server/src/presets.rs），失败回落本地兜底
  const [presetCatalog, setPresetCatalog] = useState<PresetMeta[]>(FALLBACK_HOME_PRESETS)
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [loaded, setLoaded] = useState(false)

  const load = useCallback(() => {
    let alive = true
    fetch('/api/public/site')
      .then((r) => r.json())
      .then((b) => {
        if (alive && b?.ok && b.data) {
          setTheme(b.data.theme || 'paper')
          setTemplate(b.data.template || 'default')
          setTitle(b.data.siteTitle || 'LightPress')
          setTagline(b.data.siteTagline || '')
          // 不能用 || 'duck' 兜底：那会把"留空=跟随预设"这个状态吞掉，
          // 变成一进页面就显示 duck、一保存就把它固化下来。
          setHomeTheme(b.data.homeTheme || '')
          setHomePreset(b.data.homePreset || 'classic')
          const cat = b.data.homePresets
          if (Array.isArray(cat) && cat.length) setPresetCatalog(cat as PresetMeta[])
          setLoaded(true)
        }
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [])

  useEffect(() => load(), [load])

  const patch = (fn: () => void) => {
    fn()
    setDirty(true)
  }

  const save = async () => {
    setSaving(true)
    try {
      await request('/api/admin/site', {
        method: 'PUT',
        body: JSON.stringify({
          theme,
          template,
          site_title: title,
          site_tagline: tagline,
          home_theme: homeTheme,
          home_preset: homePreset,
        }),
      })
      toast(t('settings.appearanceSaved'), { variant: 'success' })
      setDirty(false)
    } catch (err) {
      toast((err as Error).message, { variant: 'danger' })
    } finally {
      setSaving(false)
    }
  }

  const reset = () => {
    load()
    setDirty(false)
  }

  if (!loaded) {
    return (
      <div className="py-8 text-default-400 text-sm">{t('common.loading')}</div>
    )
  }

  return (
    <div className="max-w-xl space-y-8 py-2">
      {/* Theme */}
      <div>
        <h3 className="text-base font-semibold mb-1">{t('settings.appearanceTheme')}</h3>
        <p className="text-xs text-default-400 mb-4">{t('settings.appearanceThemeHint')}</p>
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
          {SITE_THEMES.map((tm) => (
            <button
              key={tm.key}
              type="button"
              onClick={() => patch(() => setTheme(tm.key))}
              className={`rounded-xl border p-3 text-left transition ${
                theme === tm.key ? 'border-primary ring-2 ring-primary/30' : 'border-default-200'
              }`}
              style={{ background: tm.vars.surface }}
            >
              <div className="flex gap-1 mb-2">
                <span className="w-4 h-4 rounded-full border" style={{ background: tm.vars.bg }} />
                <span className="w-4 h-4 rounded-full border" style={{ background: tm.vars.accent }} />
                <span className="w-4 h-4 rounded-full border" style={{ background: tm.vars.text }} />
              </div>
              <p className="text-sm font-medium" style={{ color: tm.vars.text }}>{tm.name}</p>
            </button>
          ))}
        </div>
      </div>

      {/* Template */}
      <div>
        <h3 className="text-base font-semibold mb-1">{t('settings.appearanceTemplate')}</h3>
        <p className="text-xs text-default-400 mb-4">{t('settings.appearanceTemplateDesc')}</p>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {SITE_TEMPLATES.map((tp) => (
            <button
              key={tp.key}
              type="button"
              onClick={() => patch(() => setTemplate(tp.key))}
              className={`rounded-xl border p-4 text-left transition ${
                template === tp.key ? 'border-primary ring-2 ring-primary/30' : 'border-default-200'
              }`}
            >
              <p className="text-sm font-medium">{tp.name}</p>
              <p className="text-xs text-default-400 mt-1">{tp.desc}</p>
            </button>
          ))}
        </div>
      </div>

      {/* Branding */}
      <div className="space-y-4">
        <div className="space-y-1.5">
          <Label>{t('settings.appearanceSiteTitle')}</Label>
          <Input
            value={title}
            onChange={(e: any) => patch(() => setTitle(e.target.value))}
            placeholder={t('settings.appearanceSiteTitlePlaceholder')}
          />
        </div>
        <div className="space-y-1.5">
          <Label>{t('settings.appearanceSiteTagline')}</Label>
          <Input
            value={tagline}
            onChange={(e: any) => patch(() => setTagline(e.target.value))}
            placeholder={t('settings.appearanceSiteTaglinePlaceholder')}
          />
        </div>
      </div>

      {/* 主页版式预设（Coucouya 公开主页）：决定区块顺序与骨架 */}
      <div className="space-y-3">
        <h3 className="text-base font-semibold mb-1">主页版式</h3>
        <p className="text-xs text-default-400 mb-3">
          决定首页区块的顺序与骨架（单栏 / 双栏），与下面的风格正交 —— 两者可自由组合。
        </p>
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {presetCatalog.map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => patch(() => setHomePreset(p.id))}
              className={`rounded-xl border p-3 text-left transition ${
                homePreset === p.id ? 'border-primary ring-2 ring-primary/30' : 'border-default-200'
              }`}
            >
              {/* 骨架缩略：左主列按 bars 画出行块；双栏预设右侧多一块侧栏 */}
              {/* 用内联中性灰而非 bg-default-* / h-[3px]：
                  这套 HeroUI 类在当前版本里没生效，行块会渲染出来但完全看不见。
                  rgba 中性灰在浅色/深色后台主题下都能读。 */}
              <div
                className="flex gap-1.5 rounded-lg p-2"
                style={{ height: 48, background: 'rgba(128,128,128,0.10)' }}
              >
                <div className="flex flex-1 flex-col justify-between">
                  {p.bars.map((w, i) => (
                    <span
                      key={i}
                      data-preset-bar=""
                      className="rounded-full"
                      style={{ width: `${w}%`, height: 3, background: 'rgba(128,128,128,0.45)' }}
                    />
                  ))}
                </div>
                {p.sidebar && (
                  <span
                    className="rounded"
                    style={{ width: '25%', background: 'rgba(128,128,128,0.45)' }}
                  />
                )}
              </div>
              <p className="mt-2 text-sm font-medium">{p.name}</p>
              <p className="mt-0.5 text-xs text-default-400">{p.desc}</p>
            </button>
          ))}
        </div>
      </div>

      {/* 主页风格（Coucouya 公开主页）：与本后台 theme 解耦 */}
      <div className="space-y-3">
        <h3 className="text-base font-semibold mb-1">主页风格</h3>
        <p className="text-xs text-default-400 mb-3">
          公开主页（coucouya）的配色与字体，独立于本后台站点主题。共 10 档。
        </p>
        {/* 允许"不指定风格"：留空即让位给版式预设自带的建议风格。
            少了这一项，这里一旦保存过一次具体风格，预设的建议就永远不生效了。 */}
        <button
          type="button"
          onClick={() => patch(() => setHomeTheme(''))}
          className={`mb-2 w-full rounded-xl border px-3 py-2 text-left transition ${
            homeTheme ? 'border-default-200' : 'border-primary ring-2 ring-primary/30'
          }`}
        >
          <p className="text-sm font-medium">跟随版式预设</p>
          <p className="mt-0.5 text-[11px] text-default-400">
            不指定风格，按当前版式预设的建议取值（如「杂志编辑」配大红衬线）
          </p>
        </button>
        <div className="grid grid-cols-2 sm:grid-cols-5 gap-3">
          {HOME_THEMES.map((ht) => (
            <button
              key={ht.id}
              type="button"
              onClick={() => patch(() => setHomeTheme(ht.id))}
              className={`rounded-xl border p-3 text-left transition ${
                homeTheme === ht.id ? 'border-primary ring-2 ring-primary/30' : 'border-default-200'
              }`}
            >
              <div className="flex gap-1 mb-2">
                {ht.swatch.map((c, i) => (
                  <span key={i} className="w-4 h-4 rounded-full border" style={{ background: c }} />
                ))}
              </div>
              <p className="text-sm font-medium">{ht.name}</p>
              <p className="mt-0.5 text-[10px] text-default-400">{ht.group}</p>
            </button>
          ))}
        </div>
      </div>

      <p className="text-[11px] text-default-400 -mt-4">
        首页区块 / 导航链接 / 主页置顶在本页下方独立维护，与主题设置分开保存。
      </p>

      {dirty && (
        <div className="flex gap-3 pt-2">
          <Button onPress={save} isDisabled={saving}>
            {t('common.save')}
          </Button>
          <Button variant="outline" onPress={reset}>
            {t('common.cancel')}
          </Button>
        </div>
      )}
    </div>
  )
}


// ── 首页模板 / 多端口管理 ───────────────────────────────────────
/**
 * 同一套后台内容可以并行跑在多个端口、套不同模板做测试；
 * 后台统一指定「激活模板」与「对外主端口」，各页面读取后以角标显示自身状态。
 * 实际对外路由由部署侧按 mainPort 指向，这里只做单一真相源的配置。
 */
const HOME_TEMPLATES = [
  {
    id: 'coucouya',
    name: '默认风格',
    desc: '现有 coucouya 首页版式',
    port: '5199',
    // 该模板是否支持版式预设；具体清单在渲染时从后端下发的目录读取，
    // 不在前端另存一份（单一权威 server/src/presets.rs）。
    hasPresets: true,
  },
  {
    id: 'fastshot',
    name: 'Fastshot 风格',
    desc: '全屏 Hero 版式（独立目录）',
    port: '5197',
    hasPresets: false,
  },
] as const

function HomeTemplatePanel() {
  const { t } = useTranslation()
  const { has } = usePermission()
  const canEdit = has('site.settings.update')

  // 版式清单从后端目录读（单一权威 server/src/presets.rs），失败回落本地兜底。
  // 本组件与 AppearanceBranding 是并列组件，各自取一份，不共用 state。
  const [presetIds, setPresetIds] = useState<string>(
    FALLBACK_HOME_PRESETS.map((p) => p.id).join(' / '),
  )
  useEffect(() => {
    let alive = true
    fetch('/api/public/site')
      .then((r) => r.json())
      .then((b) => {
        const cat = b && b.data ? b.data.homePresets : null
        if (alive && Array.isArray(cat) && cat.length) {
          setPresetIds(cat.map((p: PresetMeta) => p.id).join(' / '))
        }
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [])

  const [tpl, setTpl] = useState<string>('coucouya')
  const [mainPort, setMainPort] = useState<string>('5199')
  const [loaded, setLoaded] = useState(false)
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)

  const load = useCallback(() => {
    let alive = true
    fetch('/api/public/site')
      .then((r) => r.json())
      .then((b) => {
        if (!alive || !b?.ok || !b.data) return
        setTpl(b.data.homeTemplate || 'coucouya')
        setMainPort(String(b.data.mainPort || '5199'))
      })
      .catch(() => {})
      .finally(() => {
        if (alive) setLoaded(true)
      })
    return () => {
      alive = false
    }
  }, [])

  useEffect(() => load(), [load])

  const patch = (fn: () => void) => {
    fn()
    setDirty(true)
  }

  const save = async () => {
    setSaving(true)
    try {
      await request('/api/admin/site', {
        method: 'PUT',
        body: JSON.stringify({ home_template: tpl, main_port: mainPort }),
      })
      toast(t('settings.appearanceSaved'), { variant: 'success' })
      setDirty(false)
    } catch (err) {
      toast((err as Error).message, { variant: 'danger' })
    } finally {
      setSaving(false)
    }
  }

  if (!loaded) {
    return <div className="py-6 text-sm text-default-400">{t('common.loading')}</div>
  }

  // 主端口上跑的是哪个模板 —— 决定上线后访客看到什么
  const onMain = HOME_TEMPLATES.find((x) => x.port === mainPort)
  const host = typeof window !== 'undefined' ? window.location.hostname : '127.0.0.1'
  const href = (port: string) => `http://${host}:${port}/`

  return (
    <div className="space-y-4">
      <div>
        <h3 className="text-base font-semibold">首页模板</h3>
        <p className="mt-1 text-xs text-default-400">
          内容始终来自本后台；多个端口可并行跑不同模板做测试，后台统一指定激活模板与对外主端口。
        </p>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        {HOME_TEMPLATES.map((ht) => {
          const active = tpl === ht.id
          return (
            <button
              key={ht.id}
              type="button"
              disabled={!canEdit}
              onClick={() => patch(() => setTpl(ht.id))}
              className={`rounded-xl border p-3 text-left transition ${
                active ? 'border-primary ring-2 ring-primary/30' : 'border-default-200'
              } ${canEdit ? '' : 'cursor-default opacity-70'}`}
            >
              <div className="flex items-center justify-between gap-2">
                <p className="text-sm font-medium">{ht.name}</p>
                {active && (
                  <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
                    已激活
                  </span>
                )}
              </div>
              <p className="mt-1 text-[11px] text-default-400">{ht.desc}</p>
              {ht.hasPresets && presetIds && (
                <p className="mt-1.5 text-[11px] text-default-500">
                  版式预设 <span className="text-default-400">{presetIds}</span>
                </p>
              )}
              <p className="mt-2 text-[11px] text-default-500">
                测试端口{' '}
                <a
                  href={href(ht.port)}
                  target="_blank"
                  rel="noreferrer"
                  className="underline hover:text-default-700"
                  onClick={(e) => e.stopPropagation()}
                >
                  {ht.port}
                </a>
              </p>
            </button>
          )
        })}
      </div>

      <div className="space-y-1.5">
        <Label>主端口（上线后对外服务）</Label>
        <Input
          value={mainPort}
          onChange={(e: any) => patch(() => setMainPort(e.target.value.replace(/\D/g, '')))}
          placeholder="5199"
          disabled={!canEdit}
          className="max-w-[200px]"
        />
        <p className="text-[11px] text-default-400">
          {onMain ? (
            <>
              当前主端口 <b>{mainPort}</b> 对外显示：<b>{onMain.name}</b>
              {onMain.id !== tpl && '（与激活模板不一致，请确认）'}
            </>
          ) : (
            <>主端口 {mainPort} 未匹配任何模板端口，请改为 5199 或 5197。</>
          )}
        </p>
      </div>

      {!canEdit && (
        <p className="text-[11px] text-default-400">仅 Owner 可切换模板与设置主端口。</p>
      )}

      {canEdit && dirty && (
        <div className="flex gap-3 pt-1">
          <Button onPress={save} isDisabled={saving} size="sm">
            {t('common.save')}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onPress={() => {
              load()
              setDirty(false)
            }}
          >
            {t('common.cancel')}
          </Button>
        </div>
      )}
    </div>
  )
}

// ── 页面：按权限拼装各区块 ──────────────────────────────────────
function SiteAppearancePage() {
  const { has } = usePermission()

  return (
    <PageContainer title="站点外观" subtitle="主页可视化编辑 / 首页模板 / 主题 / 首页区块 / 导航链接 / 主页置顶">
      <div className="max-w-3xl space-y-8 py-2">
        {/* 真实主页预览：点元素 → 弹出对应内容的编辑框 */}
        <SiteLivePreview />

        {/* 首页模板切换 + 多端口管理 */}
        <div className="pt-6 border-t border-default-100">
          <HomeTemplatePanel />
        </div>

        {has('site.settings.update') ? (
          <AppearanceBranding />
        ) : (
          <div className="rounded-xl border border-default-200 bg-default-50 px-4 py-3 text-sm text-default-500">
            主题 / 模板 / 品牌设置仅 Owner 可修改；下面的导航与置顶区块你可正常编辑。
          </div>
        )}

        {has('content.sections.view') && (
          <div className="pt-6 border-t border-default-100">
            <HomeBlocksEditor />
          </div>
        )}

        {has('site.nav.view') && (
          <div className="pt-6 border-t border-default-100">
            <NavLinksEditor />
          </div>
        )}

        {has('site.homePins.view') && (
          <div className="pt-6 border-t border-default-100">
            <HomePinsEditor />
          </div>
        )}
      </div>
    </PageContainer>
  )
}

export const Route = createFileRoute('/site-appearance')({
  component: SiteAppearancePage,
})
