/**
 * 版块编辑弹窗（真实主页预览里点击元素后弹出）
 *
 * 数据来源由元素的 `data-edit` 决定，共四类：
 *  - `section:<slug>:<path>`  首页区块（sections 表），按字段路径精确定位到子对象
 *  - `nav:<grp>`              导航 / 页脚链接（nav_links 表）
 *  - `pins:<slot>`            主页置顶文章（home_pins 表）
 *  - `site:<key>`             站点级文案（site_settings 的 home JSON），此处只做说明
 *
 * section 走「整体保存」：编辑的是子结构的副本，保存时按路径写回 body 再 PUT；
 * nav / pins 走「行内即时保存」，与站点外观页的编辑器行为一致。
 */
import { useEffect, useMemo, useState } from 'react'
import { Button, Input, Label, Modal, Switch, toast } from '@heroui/react'
import { api, request } from '../../api/client'
import { JsonForm } from './JsonForm'

export type EditKind = 'section' | 'nav' | 'pins' | 'site'

export interface EditTarget {
  kind: EditKind
  /** section：区块 slug */
  slug?: string
  /** section：body 内的字段路径，如 `hero.title` */
  path?: string
  /** nav：nav | footer */
  group?: string
  /** pins：writing | series */
  slot?: string
  /** site：键名 */
  key?: string
  label: string
  text: string
}

/** 解析 `kind:ref` 形式的 data-edit 值 */
export function parseEditRef(ref: string, label = '', text = ''): EditTarget | null {
  const [kind, ...rest] = (ref || '').split(':')
  if (kind === 'section') {
    const [slug, ...p] = rest
    if (!slug) return null
    return { kind: 'section', slug, path: p.join('.'), label, text }
  }
  if (kind === 'nav') return { kind: 'nav', group: rest[0] || 'nav', label, text }
  if (kind === 'pins') return { kind: 'pins', slot: rest[0] || 'writing', label, text }
  if (kind === 'site') return { kind: 'site', key: rest[0] || '', label, text }
  return null
}

interface SectionRow {
  id: string
  slug: string
  title: string
  subtitle: string | null
  body: unknown
}

interface NavLinkRow {
  id: string
  grp: string
  label: string
  href: string
  target: string
  sort: number
  enabled: boolean
}

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

const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v ?? null)) as T

/** 取值：path 为空表示整块 */
function getPath(obj: unknown, path: string): unknown {
  if (!path) return obj
  return path
    .split('.')
    .reduce<unknown>((acc, k) => (acc == null ? undefined : (acc as Record<string, unknown>)[k]), obj)
}

/** 不可变写回：只替换 path 指向的子树 */
function setPath(obj: unknown, path: string, value: unknown): unknown {
  if (!path) return value
  const [head, ...rest] = path.split('.')
  const base = (obj && typeof obj === 'object' ? obj : {}) as Record<string, unknown>
  return {
    ...base,
    [head]: rest.length ? setPath(base[head], rest.join('.'), value) : value,
  }
}

interface Props {
  target: EditTarget | null
  onClose: () => void
  /** 保存完成后通知预览刷新 */
  onSaved: () => void
}

export function ZoneEditDialog({ target, onClose, onSaved }: Props) {
  const [loading, setLoading] = useState(false)
  const [err, setErr] = useState('')
  const [saving, setSaving] = useState(false)

  const [section, setSection] = useState<SectionRow | null>(null)
  const [draft, setDraft] = useState<unknown>(null)
  const [nav, setNav] = useState<NavLinkRow[]>([])
  const [pins, setPins] = useState<HomePinRow[]>([])
  const [articles, setArticles] = useState<ArticleBrief[]>([])
  const [draftArticle, setDraftArticle] = useState('')

  const isOpen = !!target

  // 用字符串 key 做依赖，避免 target 对象每次点击都是新引用导致重复拉取
  const key = target
    ? [target.kind, target.slug, target.path, target.group, target.slot, target.key].join('|')
    : ''

  useEffect(() => {
    if (!target) return
    let alive = true
    setLoading(true)
    setErr('')
    setDraftArticle('')

    void (async () => {
      try {
        if (target.kind === 'section') {
          const rows = await api<SectionRow[]>('/api/sections')
          const row = (rows || []).find((r) => r.slug === target.slug)
          if (!row) throw new Error(`未找到区块「${target.slug}」，请先启动新版后台生成默认区块`)
          if (!alive) return
          setSection(row)
          setDraft(clone(getPath(row.body, target.path || '')))
        } else if (target.kind === 'nav') {
          const rows = await api<NavLinkRow[]>('/api/navlinks')
          const list = (rows || [])
            .filter((r) => r.grp === target.group)
            .sort((a, b) => a.sort - b.sort)
          if (!alive) return
          setNav(list)
        } else if (target.kind === 'pins') {
          const [prows, arts] = await Promise.all([
            api<HomePinRow[]>('/api/home_pins'),
            api<ArticleBrief[]>('/api/articles'),
          ])
          if (!alive) return
          setPins(
            (prows || [])
              .filter((r) => r.slot === target.slot)
              .sort((a, b) => a.sort - b.sort),
          )
          setArticles((arts || []).filter((a) => a.status === 'published'))
        }
      } catch (e) {
        if (alive) setErr((e as Error).message)
      } finally {
        if (alive) setLoading(false)
      }
    })()

    return () => {
      alive = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key])

  const modalState = useMemo(
    () => ({ isOpen, setOpen: onClose, open: () => {}, close: onClose, toggle: () => {} }),
    [isOpen, onClose],
  )

  // ── nav 行内操作（即时保存）──
  const patchNav = async (row: NavLinkRow, patchBody: Partial<NavLinkRow>) => {
    await request(`/api/navlinks/${row.id}`, {
      method: 'PUT',
      body: JSON.stringify(patchBody),
    })
    setNav((prev) => prev.map((r) => (r.id === row.id ? { ...r, ...patchBody } : r)))
    onSaved()
  }

  const moveNav = (row: NavLinkRow, dir: -1 | 1) => {
    const idx = nav.findIndex((r) => r.id === row.id)
    const swap = nav[idx + dir]
    if (!swap) return
    const a = row.sort
    const b = swap.sort
    void (async () => {
      try {
        await patchNav(row, { sort: -1 })
        await patchNav(swap, { sort: a })
        await patchNav(row, { sort: b })
        // 本地同步顺序，避免等接口回来才看到变化
        setNav((prev) => {
          const next = [...prev]
          const i = next.findIndex((r) => r.id === row.id)
          const j = next.findIndex((r) => r.id === swap.id)
          ;[next[i], next[j]] = [next[j], next[i]]
          return next.map((r, k) => ({ ...r, sort: k + 1 }))
        })
      } catch (e) {
        toast((e as Error).message, { variant: 'danger' })
      }
    })()
  }

  const removeNav = async (row: NavLinkRow) => {
    try {
      await request(`/api/navlinks/${row.id}`, { method: 'DELETE' })
      setNav((prev) => prev.filter((r) => r.id !== row.id))
      onSaved()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    }
  }

  /**
   * 用站点内置锚点初始化导航。
   * nav_links 表为空时，页面会回落到站点内置的默认锚点；
   * 加第一条后即改由表驱动，所以这里给出一键补齐，避免面对空列表无从下手。
   */
  const seedNav = async () => {
    if (!target || target.kind !== 'nav') return
    const preset =
      target.group === 'footer'
        ? [
            { label: 'X / Twitter', href: 'https://x.com/KeKeYa88', target: '_blank' },
            { label: 'KOSX.ai', href: 'https://kosx.ai/', target: '_blank' },
          ]
        : [
            { label: '关于', href: '#about', target: '' },
            { label: '组织', href: '#affiliation', target: '' },
            { label: '文章', href: '#writing', target: '' },
            { label: '实验', href: '#series', target: '' },
            { label: 'Web3', href: '#web3', target: '' },
          ]
    try {
      for (let i = 0; i < preset.length; i++) {
        await request('/api/navlinks', {
          method: 'POST',
          body: JSON.stringify({ ...preset[i], grp: target.group, sort: i + 1, enabled: true }),
        })
      }
      const rows = await api<NavLinkRow[]>('/api/navlinks')
      setNav(
        (rows || [])
          .filter((r) => r.grp === target.group)
          .sort((a, b) => a.sort - b.sort),
      )
      onSaved()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    }
  }

  const addNav = async () => {
    if (!target || target.kind !== 'nav') return
    const maxSort = Math.max(0, ...nav.map((r) => r.sort))
    try {
      const res = await request('/api/navlinks', {
        method: 'POST',
        body: JSON.stringify({
          grp: target.group,
          label: '新链接',
          href: '#',
          target: '',
          sort: maxSort + 1,
          enabled: true,
        }),
      })
      const created = (res as { data?: NavLinkRow })?.data
      if (created?.id) setNav((prev) => [...prev, { ...created, sort: maxSort + 1 }])
      onSaved()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    }
  }

  // ── pins 行内操作（即时保存）──
  const removePin = async (row: HomePinRow) => {
    try {
      await request(`/api/home_pins/${row.id}`, { method: 'DELETE' })
      setPins((prev) => prev.filter((r) => r.id !== row.id))
      onSaved()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    }
  }

  const togglePin = async (row: HomePinRow) => {
    try {
      await request(`/api/home_pins/${row.id}`, {
        method: 'PUT',
        body: JSON.stringify({ enabled: !row.enabled }),
      })
      setPins((prev) => prev.map((r) => (r.id === row.id ? { ...r, enabled: !row.enabled } : r)))
      onSaved()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    }
  }

  const addPin = async () => {
    if (!target || target.kind !== 'pins' || !draftArticle) return
    if (pins.some((r) => r.articleId === draftArticle)) {
      toast('该文章已在当前展示位中', { variant: 'warning' })
      return
    }
    const maxSort = Math.max(0, ...pins.map((r) => r.sort))
    try {
      const res = await request('/api/home_pins', {
        method: 'POST',
        body: JSON.stringify({
          slot: target.slot,
          articleId: draftArticle,
          sort: maxSort + 1,
          enabled: true,
        }),
      })
      const created = (res as { data?: HomePinRow })?.data
      if (created?.id) setPins((prev) => [...prev, { ...created, sort: maxSort + 1 }])
      setDraftArticle('')
      onSaved()
    } catch (e) {
      toast((e as Error).message, { variant: 'danger' })
    }
  }

  // ── 保存（section 需要整体提交；nav/pins 已即时保存）──
  const save = async () => {
    if (!target) return
    if (target.kind === 'section') {
      if (!section) return
      setSaving(true)
      try {
        const body = setPath(section.body, target.path || '', draft)
        await request(`/api/sections/${section.id}`, {
          method: 'PUT',
          body: JSON.stringify({ title: section.title, subtitle: section.subtitle, body }),
        })
        toast('已保存，预览已刷新', { variant: 'success' })
        onSaved()
        onClose()
      } catch (e) {
        toast((e as Error).message, { variant: 'danger' })
      } finally {
        setSaving(false)
      }
      return
    }
    // nav / pins：所有改动已逐条落库，这里只负责刷新预览并关闭
    onSaved()
    onClose()
  }

  const title = target?.label || target?.text || '编辑内容'
  const subtitle = useMemo(() => {
    if (!target) return ''
    if (target.kind === 'section') return `首页区块 · ${target.slug}${target.path ? ` · ${target.path}` : ''}`
    if (target.kind === 'nav') return `nav_links 表 · ${target.group === 'footer' ? '页脚链接' : '顶部导航'}`
    if (target.kind === 'pins') return `home_pins 表 · ${target.slot === 'series' ? '系列' : '代表文章'}`
    return 'site_settings · 站点设置'
  }, [target])

  return (
    <Modal state={modalState}>
      <Modal.Backdrop>
        <Modal.Container placement="center">
          <Modal.Dialog className="w-[620px] max-w-[95vw]">
            <Modal.Header>
              <div>
                <Modal.Heading>{title}</Modal.Heading>
                <p className="mt-0.5 font-mono text-[11px] text-default-400">{subtitle}</p>
              </div>
              <Modal.CloseTrigger />
            </Modal.Header>

            <Modal.Body>
              <div className="max-h-[62vh] space-y-3 overflow-y-auto pr-1">
                {loading && <p className="py-6 text-sm text-default-400">加载中…</p>}
                {err && (
                  <p className="rounded-lg bg-danger-50 px-3 py-2 text-sm text-danger-600">{err}</p>
                )}

                {/* 首页区块：结构化字段表单 */}
                {!loading && !err && target?.kind === 'section' && (
                  <>
                    {target.path && (
                      <p className="text-[11px] text-default-400">
                        当前编辑「{target.label || target.path}」这一段，下方字段即页面上的内容。
                      </p>
                    )}
                    <div className="rounded-lg border border-default-200 bg-default-50/60 p-3">
                      <JsonForm value={draft} onChange={setDraft} />
                    </div>
                  </>
                )}

                {/* 导航 / 页脚链接 */}
                {!loading && !err && target?.kind === 'nav' && (
                  <div className="space-y-2">
                    {nav.length === 0 && (
                      <div className="space-y-2 rounded-lg border border-dashed border-default-300 p-3">
                        <p className="text-sm text-default-500">
                          这一组还没有链接，页面当前显示的是站点内置的默认锚点。
                        </p>
                        <p className="text-[11px] text-default-400">
                          补上第一条后，导航就改由这里的 nav_links 表驱动。
                        </p>
                        <Button size="sm" variant="outline" onPress={() => void seedNav()}>
                          用默认锚点初始化
                        </Button>
                      </div>
                    )}
                    {nav.map((row) => (
                      <div
                        key={row.id}
                        className="flex items-start gap-2 rounded-lg border border-default-200 bg-default-50/60 px-2.5 py-2"
                      >
                        <div className="flex flex-col gap-0.5 pt-1.5">
                          <button
                            type="button"
                            aria-label="上移"
                            className="text-[10px] text-default-400 hover:text-default-700"
                            onClick={() => moveNav(row, -1)}
                          >
                            ▲
                          </button>
                          <button
                            type="button"
                            aria-label="下移"
                            className="text-[10px] text-default-400 hover:text-default-700"
                            onClick={() => moveNav(row, 1)}
                          >
                            ▼
                          </button>
                        </div>
                        <div className="flex-1 space-y-1.5">
                          <Input
                            aria-label="名称"
                            value={row.label}
                            onChange={(e: any) =>
                              setNav((prev) =>
                                prev.map((r) => (r.id === row.id ? { ...r, label: e.target.value } : r)),
                              )
                            }
                            onBlur={() =>
                              row.label.trim() &&
                              void patchNav(row, { label: row.label.trim() }).catch((e: Error) =>
                                toast(e.message, { variant: 'danger' }),
                              )
                            }
                            placeholder="名称"
                          />
                          <Input
                            aria-label="地址"
                            value={row.href}
                            onChange={(e: any) =>
                              setNav((prev) =>
                                prev.map((r) => (r.id === row.id ? { ...r, href: e.target.value } : r)),
                              )
                            }
                            onBlur={() =>
                              row.href.trim() &&
                              void patchNav(row, { href: row.href.trim() }).catch((e: Error) =>
                                toast(e.message, { variant: 'danger' }),
                              )
                            }
                            placeholder="#about / /tag/xxx / https://…"
                          />
                          <div className="flex items-center gap-3">
                            <label className="flex items-center gap-1.5 text-xs text-default-500">
                              <input
                                type="checkbox"
                                checked={row.target === '_blank'}
                                onChange={(e) =>
                                  void patchNav(row, {
                                    target: e.target.checked ? '_blank' : '',
                                  }).catch((err: Error) => toast(err.message, { variant: 'danger' }))
                                }
                              />
                              新窗口
                            </label>
                            <Switch
                              size="sm"
                              isSelected={row.enabled}
                              onChange={() =>
                                void patchNav(row, { enabled: !row.enabled }).catch((e: Error) =>
                                  toast(e.message, { variant: 'danger' }),
                                )
                              }
                              aria-label="启用"
                            />
                            <Button
                              size="sm"
                              variant="ghost"
                              onPress={() => void removeNav(row)}
                              className="ml-auto"
                            >
                              删除
                            </Button>
                          </div>
                        </div>
                      </div>
                    ))}
                    <Button size="sm" variant="outline" onPress={() => void addNav()}>
                      + 新增链接
                    </Button>
                  </div>
                )}

                {/* 主页置顶文章 */}
                {!loading && !err && target?.kind === 'pins' && (
                  <div className="space-y-2">
                    {pins.length === 0 && (
                      <p className="py-2 text-sm text-default-400">
                        未配置，主页当前回落展示 featured 文章。
                      </p>
                    )}
                    {pins.map((row) => (
                      <div
                        key={row.id}
                        className="flex items-center gap-2 rounded-lg border border-default-200 bg-default-50/60 px-2.5 py-2"
                      >
                        <span className="flex-1 truncate text-sm">
                          {articles.find((a) => a.id === row.articleId)?.title ||
                            `（未知文章 ${row.articleId.slice(0, 8)}）`}
                        </span>
                        <Switch
                          size="sm"
                          isSelected={row.enabled}
                          onChange={() => void togglePin(row)}
                          aria-label="启用"
                        />
                        <Button size="sm" variant="ghost" onPress={() => void removePin(row)}>
                          移除
                        </Button>
                      </div>
                    ))}
                    <div className="flex items-center gap-2 pt-1">
                      <select
                        value={draftArticle}
                        onChange={(e) => setDraftArticle(e.target.value)}
                        className="min-w-0 flex-1 rounded-lg border border-default-200 bg-default-50 px-2 py-1.5 text-sm outline-none focus:border-primary"
                      >
                        <option value="">选择已发布文章…</option>
                        {articles.map((a) => (
                          <option key={a.id} value={a.id}>
                            {a.title || a.id}
                          </option>
                        ))}
                      </select>
                      <Button size="sm" onPress={() => void addPin()}>
                        置顶
                      </Button>
                    </div>
                  </div>
                )}

                {/* 站点级文案：暂无可视化编辑器，说明来源 */}
                {!loading && !err && target?.kind === 'site' && (
                  <div className="space-y-2">
                    <p className="text-sm text-default-600">
                      这一段来自站点设置里的首页 JSON（site_settings 的 home 键），
                      目前还没有结构化编辑器。
                    </p>
                    {target.text && (
                      <div className="rounded-lg border border-default-200 bg-default-50 p-3">
                        <Label className="text-xs">页面上的内容</Label>
                        <p className="mt-1 whitespace-pre-wrap text-[13px] leading-relaxed text-default-700">
                          {target.text}
                        </p>
                      </div>
                    )}
                    <p className="text-[11px] text-default-400">
                      可编辑的区块（导航 / Hero / 组织 / 系列 / Web3 / 置顶文章）在页面上都有蓝色描边，
                      鼠标移上去即可看到。
                    </p>
                  </div>
                )}
              </div>
            </Modal.Body>

            <Modal.Footer>
              <Button variant="outline" onPress={onClose}>
                关闭
              </Button>
              {/* 加载失败时没有可操作的数据，不给保存按钮（避免误导） */}
              {target?.kind !== 'site' && !err && (
                <Button onPress={() => void save()} isDisabled={saving || loading}>
                  {target?.kind === 'section' ? '保存' : '完成'}
                </Button>
              )}
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  )
}
